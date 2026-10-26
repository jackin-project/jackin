use std::collections::HashSet;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use super::StoreError;
use super::manifest::{
    create_generation_dir, create_private_file, manifest_bytes, write_immutable,
};
use super::path::{FileIdentity, ResolvedPath, identity_fields, path_digest, sidecar_path};

#[derive(Clone)]
pub(super) struct PreopenEvidence {
    pub source_existed: bool,
    pub source_identity: Option<FileIdentity>,
    pub directory: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest_identity: Option<FileIdentity>,
    pub manifest_sha256: String,
    pub generation: String,
}

type SourcePath = (PathBuf, PathBuf, &'static str);

struct CapturePlan {
    path_hash: String,
    directory: PathBuf,
    generation: String,
    source_paths: [SourcePath; 3],
    initial: Vec<Option<FileStamp>>,
    source_identities: HashSet<FileIdentity>,
    identity_lease: super::registry::IdentityLease,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileStamp {
    identity: Option<FileIdentity>,
    length: u64,
    modified_ns: Option<u128>,
    #[cfg(unix)]
    changed_ns: i128,
}

#[derive(Clone, Debug)]
struct FileRecord {
    name: &'static str,
    exists: bool,
    stamp: Option<FileStamp>,
    sha256: Option<String>,
    artifact_identity: Option<FileIdentity>,
}

pub(super) fn capture_preopen(
    resolved: &ResolvedPath,
    permit: &super::registry::ManagedOpenPermit,
) -> Result<
    (
        PreopenEvidence,
        HashSet<FileIdentity>,
        super::registry::IdentityLease,
    ),
    StoreError,
> {
    let plan = prepare_preopen_capture(resolved, permit)?;
    let records = capture_source_records(&plan, resolved)?;
    let evidence = publish_preopen_evidence(resolved, &plan, &records)?;
    Ok((evidence, plan.source_identities, plan.identity_lease))
}

fn prepare_preopen_capture(
    resolved: &ResolvedPath,
    permit: &super::registry::ManagedOpenPermit,
) -> Result<CapturePlan, StoreError> {
    if !resolved.parent.is_dir() {
        return Err(StoreError::UnsupportedSourcePath);
    }
    let path_hash = path_digest(&resolved.path);
    let (directory, generation) = create_generation_dir(&resolved.parent, &path_hash)?;
    write_immutable(
        &directory,
        "capture_incomplete.manifest",
        &manifest_bytes(&[
            ("format".to_owned(), "jackin-quota-forensic-v1".to_owned()),
            ("status".to_owned(), "forensic_incomplete".to_owned()),
            ("generation".to_owned(), generation.clone()),
            ("source_path_sha256".to_owned(), path_hash.clone()),
        ]),
    )?;
    let source_paths = [
        (resolved.path.clone(), directory.join("source.main"), "main"),
        (
            sidecar_path(&resolved.path, "-wal"),
            directory.join("source.wal"),
            "wal",
        ),
        (
            sidecar_path(&resolved.path, "-journal"),
            directory.join("source.journal"),
            "journal",
        ),
    ];
    let initial = source_paths
        .iter()
        .map(|(source, _, _)| file_stamp(source))
        .collect::<Result<Vec<_>, _>>()?;
    if initial[0].as_ref().and_then(|stamp| stamp.identity) != resolved.identity {
        return Err(StoreError::SourceIdentityChanged);
    }
    let source_identities = initial
        .iter()
        .filter_map(|stamp| stamp.as_ref().and_then(|stamp| stamp.identity))
        .collect::<HashSet<_>>();
    let identity_lease =
        super::registry::reserve_capture_identities(permit, source_identities.iter().copied())?;
    Ok(CapturePlan {
        path_hash,
        directory,
        generation,
        source_paths,
        initial,
        source_identities,
        identity_lease,
    })
}

fn capture_source_records(
    plan: &CapturePlan,
    resolved: &ResolvedPath,
) -> Result<Vec<FileRecord>, StoreError> {
    let records = plan
        .source_paths
        .iter()
        .zip(plan.initial.iter())
        .map(|((source, destination, name), stamp)| {
            capture_one(source, destination, name, &plan.path_hash, stamp.as_ref())
        })
        .collect::<Result<Vec<_>, _>>()?;
    for ((source, _, _), before) in plan.source_paths.iter().zip(plan.initial.iter()) {
        if file_stamp(source)? != *before {
            return Err(StoreError::SourceChangedDuringCapture);
        }
    }

    let source_existed = records[0].exists;
    if !source_existed && records.iter().skip(1).any(|record| record.exists) {
        return Err(StoreError::SourceChangedDuringCapture);
    }
    if records[0].stamp.as_ref().and_then(|stamp| stamp.identity) != resolved.identity {
        return Err(StoreError::SourceIdentityChanged);
    }
    Ok(records)
}

fn publish_preopen_evidence(
    resolved: &ResolvedPath,
    plan: &CapturePlan,
    records: &[FileRecord],
) -> Result<PreopenEvidence, StoreError> {
    let source_existed = records[0].exists;
    let (device, inode) = identity_fields(resolved.identity);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::ClockUnavailable)?
        .as_nanos()
        .to_string();
    let mut fields = vec![
        ("format".to_owned(), "jackin-quota-forensic-v1".to_owned()),
        (
            "status".to_owned(),
            "forensic_preopen_noncoherent".to_owned(),
        ),
        ("generation".to_owned(), plan.generation.clone()),
        ("source_path_sha256".to_owned(), plan.path_hash.clone()),
        ("source_device".to_owned(), device),
        ("source_inode".to_owned(), inode),
        ("source_existed".to_owned(), source_existed.to_string()),
        ("captured_unix_ns".to_owned(), now),
    ];
    fields.extend(forensic_record_fields(records));
    let manifest = manifest_bytes(&fields);
    let manifest_sha256 = hex::encode(Sha256::digest(&manifest));
    let manifest_path = write_immutable(&plan.directory, "preopen.manifest", &manifest)?;
    let manifest_identity = super::manifest::private_file_identity(&manifest_path)?;
    Ok(PreopenEvidence {
        source_existed,
        source_identity: resolved.identity,
        directory: plan.directory.clone(),
        manifest_path,
        manifest_identity,
        manifest_sha256,
        generation: plan.generation.clone(),
    })
}

fn forensic_record_fields(records: &[FileRecord]) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    for record in records {
        let prefix = format!("source_{}", record.name);
        fields.push((format!("{prefix}_exists"), record.exists.to_string()));
        fields.push((
            format!("{prefix}_length"),
            record
                .stamp
                .as_ref()
                .map_or_else(|| "absent".to_owned(), |stamp| stamp.length.to_string()),
        ));
        fields.push((
            format!("{prefix}_sha256"),
            record.sha256.clone().unwrap_or_else(|| "absent".to_owned()),
        ));
        if let Some(stamp) = &record.stamp {
            let (device, inode) = identity_fields(stamp.identity);
            fields.push((format!("{prefix}_device"), device));
            fields.push((format!("{prefix}_inode"), inode));
            fields.push((
                format!("{prefix}_modified_unix_ns"),
                stamp
                    .modified_ns
                    .map_or_else(|| "unknown".to_owned(), |value| value.to_string()),
            ));
            #[cfg(unix)]
            fields.push((
                format!("{prefix}_changed_unix_ns"),
                stamp.changed_ns.to_string(),
            ));
        }
        if record.exists {
            let (device, inode) = identity_fields(record.artifact_identity);
            fields.push((format!("artifact_{}_device", record.name), device));
            fields.push((format!("artifact_{}_inode", record.name), inode));
        }
    }
    fields
}

fn capture_one(
    source_path: &Path,
    destination_path: &Path,
    name: &'static str,
    path_hash: &str,
    before: Option<&FileStamp>,
) -> Result<FileRecord, StoreError> {
    let Some(before_stamp) = before else {
        return Ok(FileRecord {
            name,
            exists: false,
            stamp: None,
            sha256: None,
            artifact_identity: None,
        });
    };

    if file_stamp(source_path)? != Some(before_stamp.clone()) {
        return Err(StoreError::SourceChangedDuringCapture);
    }

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::fcntl::OFlag::O_NOFOLLOW.bits());
    }
    fail_capture_at(path_hash, CaptureFaultPhase::SourceOpen)?;
    #[expect(
        clippy::disallowed_methods,
        reason = "all source-file capture runs inside spawn_blocking before backend open"
    )]
    let mut source = options
        .open(source_path)
        .map_err(|source| StoreError::io("open preopen source file", source))?;
    record_event(path_hash, CaptureEvent::SourceOpened);
    let result = capture_open_file(
        &mut source,
        source_path,
        destination_path,
        before_stamp,
        path_hash,
    );
    drop(source);
    record_event(path_hash, CaptureEvent::SourceClosed);
    let sha256 = result?;
    let artifact_metadata = fs::symlink_metadata(destination_path)
        .map_err(|source| StoreError::io("inspect forensic artifact", source))?;
    if artifact_metadata.file_type().is_symlink() || !artifact_metadata.file_type().is_file() {
        return Err(StoreError::BackupIncomplete);
    }
    if artifact_metadata.len() != before_stamp.length {
        return Err(StoreError::BackupIncomplete);
    }
    let artifact_identity = FileIdentity::from_metadata(&artifact_metadata);
    if artifact_identity.is_some() && artifact_identity == before_stamp.identity {
        return Err(StoreError::BackupIncomplete);
    }
    Ok(FileRecord {
        name,
        exists: true,
        stamp: Some(before_stamp.clone()),
        sha256: Some(sha256),
        artifact_identity,
    })
}

fn capture_open_file(
    source: &mut File,
    source_path: &Path,
    destination_path: &Path,
    before: &FileStamp,
    path_hash: &str,
) -> Result<String, StoreError> {
    if stamp_from_metadata(
        &source
            .metadata()
            .map_err(|source| StoreError::io("inspect open source file", source))?,
    ) != *before
    {
        return Err(StoreError::SourceChangedDuringCapture);
    }
    let source_hash = hash_file(source, path_hash)?;
    verify_unchanged(source, source_path, before)?;
    fail_capture_at(path_hash, CaptureFaultPhase::CopySeek)?;
    source
        .seek(SeekFrom::Start(0))
        .map_err(|source| StoreError::io("rewind preopen source file", source))?;
    let mut destination = create_private_file(destination_path)?;
    fail_capture_at(path_hash, CaptureFaultPhase::ArtifactPermission)?;
    let copied_hash = copy_and_hash(source, &mut destination, path_hash)?;
    fail_capture_at(path_hash, CaptureFaultPhase::ArtifactSync)?;
    destination
        .sync_all()
        .map_err(|source| StoreError::io("sync forensic artifact", source))?;
    drop(destination);
    verify_unchanged(source, source_path, before)?;
    if source_hash != copied_hash {
        return Err(StoreError::SourceChangedDuringCapture);
    }
    Ok(hex::encode(source_hash))
}

fn hash_file(file: &mut File, path_hash: &str) -> Result<[u8; 32], StoreError> {
    fail_capture_at(path_hash, CaptureFaultPhase::HashSeek)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|source| StoreError::io("rewind source for hash", source))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        fail_capture_at(path_hash, CaptureFaultPhase::HashRead)?;
        let read = file
            .read(&mut buffer)
            .map_err(|source| StoreError::io("read source for hash", source))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

fn copy_and_hash(
    source: &mut File,
    destination: &mut File,
    path_hash: &str,
) -> Result<[u8; 32], StoreError> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        fail_capture_at(path_hash, CaptureFaultPhase::CopyRead)?;
        let read = source
            .read(&mut buffer)
            .map_err(|source| StoreError::io("copy forensic source", source))?;
        if read == 0 {
            break;
        }
        fail_capture_at(path_hash, CaptureFaultPhase::ArtifactWrite)?;
        destination
            .write_all(&buffer[..read])
            .map_err(|source| StoreError::io("write forensic artifact", source))?;
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

fn verify_unchanged(file: &File, path: &Path, expected: &FileStamp) -> Result<(), StoreError> {
    let handle = file
        .metadata()
        .map_err(|source| StoreError::io("recheck open source file", source))?;
    let path_stamp = file_stamp(path)?.ok_or(StoreError::SourceChangedDuringCapture)?;
    if stamp_from_metadata(&handle) != *expected || path_stamp != *expected {
        return Err(StoreError::SourceChangedDuringCapture);
    }
    Ok(())
}

fn file_stamp(path: &Path) -> Result<Option<FileStamp>, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
                return Err(StoreError::UnsafeSourceFile);
            }
            if FileIdentity::is_multiply_linked(&metadata) {
                return Err(StoreError::HardLinkedSourceFile);
            }
            Ok(Some(stamp_from_metadata(&metadata)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(StoreError::io("inspect preopen source file", source)),
    }
}

fn stamp_from_metadata(metadata: &Metadata) -> FileStamp {
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());
    #[cfg(unix)]
    let changed_ns = {
        use std::os::unix::fs::MetadataExt;
        i128::from(metadata.ctime()) * 1_000_000_000 + i128::from(metadata.ctime_nsec())
    };
    FileStamp {
        identity: FileIdentity::from_metadata(metadata),
        length: metadata.len(),
        modified_ns,
        #[cfg(unix)]
        changed_ns,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CaptureEvent {
    SourceOpened,
    SourceClosed,
    BuilderStarted,
    ArtifactOpened(Option<FileIdentity>),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum CaptureFaultPhase {
    SourceOpen,
    HashSeek,
    HashRead,
    CopySeek,
    CopyRead,
    ArtifactWrite,
    ArtifactPermission,
    ArtifactSync,
}

fn fail_capture_at(_path_hash: &str, _phase: CaptureFaultPhase) -> Result<(), StoreError> {
    Ok(())
}

fn record_event(_path_hash: &str, _event: CaptureEvent) {}

pub(super) fn record_builder_started(path: &Path) {
    record_event(&path_digest(path), CaptureEvent::BuilderStarted);
}

pub(super) fn record_artifact_open(path: &Path, identity: Option<FileIdentity>) {
    record_event(&path_digest(path), CaptureEvent::ArtifactOpened(identity));
}
