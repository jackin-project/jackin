use std::collections::BTreeMap;

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use super::path::FileIdentity;
use crate::local_store::StoreError;

static GENERATION: AtomicU64 = AtomicU64::new(1);

pub(super) async fn run_blocking<T, F>(action: &'static str, work: F) -> Result<T, StoreError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, StoreError> + Send + 'static,
{
    jackin_telemetry::spawn::joined_blocking(work)
        .await
        .map_err(|_| StoreError::Backend(action))?
}

pub(super) async fn create_generation_dir_on_worker(
    parent: &Path,
    path_hash: &str,
) -> Result<(PathBuf, String), StoreError> {
    let parent = parent.to_path_buf();
    let path_hash = path_hash.to_owned();
    run_blocking("create backup generation task failed", move || {
        create_generation_dir(&parent, &path_hash)
    })
    .await
}

pub(super) async fn write_immutable_on_worker(
    directory: &Path,
    final_name: &'static str,
    contents: Vec<u8>,
) -> Result<PathBuf, StoreError> {
    let directory = directory.to_path_buf();
    run_blocking("publish backup manifest task failed", move || {
        write_immutable(&directory, final_name, &contents)
    })
    .await
}

pub(super) async fn read_manifest_on_worker(
    path: &Path,
) -> Result<BTreeMap<String, String>, StoreError> {
    let path = path.to_path_buf();
    run_blocking("read backup manifest task failed", move || {
        read_manifest(&path)
    })
    .await
}

pub(super) async fn sha256_file_on_worker_expected(
    path: &Path,
    expected_identity: Option<FileIdentity>,
) -> Result<(String, u64), StoreError> {
    let path = path.to_path_buf();
    run_blocking("hash backup artifact task failed", move || {
        sha256_file_expected(&path, expected_identity)
    })
    .await
}

pub(super) async fn sync_file_on_worker(path: &Path) -> Result<(), StoreError> {
    let path = path.to_path_buf();
    run_blocking("sync backup artifact task failed", move || sync_file(&path)).await
}

pub(super) fn create_generation_dir(
    parent: &Path,
    path_hash: &str,
) -> Result<(PathBuf, String), StoreError> {
    for _ in 0..128 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| StoreError::ClockUnavailable)?;
        let counter = GENERATION.fetch_add(1, Ordering::Relaxed);
        let id = format!(
            "{:x}-{:x}-{:x}",
            std::process::id(),
            now.as_nanos(),
            counter
        );
        let name = format!(".jackin-quota-backup-{}-{}", &path_hash[..16], id);
        let directory = parent.join(name);
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&directory) {
            Ok(()) => {
                #[cfg(unix)]
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).map_err(
                    |source| StoreError::io("restrict private backup directory", source),
                )?;
                sync_directory(parent)?;
                return Ok((directory, id));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(source) => return Err(StoreError::io("create private backup directory", source)),
        }
    }
    Err(StoreError::GenerationUnavailable)
}

pub(super) fn write_immutable(
    directory: &Path,
    final_name: &str,
    contents: &[u8],
) -> Result<PathBuf, StoreError> {
    let final_path = directory.join(final_name);
    match fs::symlink_metadata(&final_path) {
        Ok(_) => return Err(StoreError::BackupCollision),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(StoreError::io(
                "inspect backup manifest destination",
                source,
            ));
        }
    }
    let temp_path = directory.join(format!(
        ".{final_name}.{}.tmp",
        GENERATION.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "manifest file creation runs inside manifest::run_blocking"
    )]
    let mut file = options
        .open(&temp_path)
        .map_err(|source| StoreError::io("create backup manifest", source))?;
    #[cfg(unix)]
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|source| StoreError::io("restrict backup manifest", source))?;
    file.write_all(contents)
        .map_err(|source| StoreError::io("write backup manifest", source))?;
    file.sync_all()
        .map_err(|source| StoreError::io("sync backup manifest", source))?;
    drop(file);
    fs::hard_link(&temp_path, &final_path)
        .map_err(|source| StoreError::io("publish immutable backup manifest", source))?;
    fs::remove_file(&temp_path)
        .map_err(|source| StoreError::io("remove manifest staging link", source))?;
    sync_directory(directory)?;
    Ok(final_path)
}

pub(super) fn create_private_file(path: &Path) -> Result<File, StoreError> {
    let mut options = OpenOptions::new();
    options.write(true).read(false).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "forensic artifact creation runs inside manifest::run_blocking"
    )]
    let file = options
        .open(path)
        .map_err(|source| StoreError::io("create private backup artifact", source))?;
    #[cfg(unix)]
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|source| StoreError::io("restrict private backup artifact", source))?;
    Ok(file)
}

pub(super) fn require_private_directory(path: &Path) -> Result<(), StoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| StoreError::io("inspect private backup directory", source))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(StoreError::InvalidManifest);
    }
    #[cfg(unix)]
    {
        if metadata.permissions().mode() & 0o777 != 0o700 {
            return Err(StoreError::InvalidManifest);
        }
    }
    Ok(())
}

pub(super) fn require_private_regular_file(path: &Path) -> Result<(), StoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| StoreError::io("inspect private backup file", source))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(StoreError::InvalidManifest);
    }
    if FileIdentity::is_multiply_linked(&metadata) {
        return Err(StoreError::InvalidManifest);
    }
    #[cfg(unix)]
    {
        if metadata.permissions().mode() & 0o777 != 0o600 {
            return Err(StoreError::InvalidManifest);
        }
    }
    Ok(())
}

pub(super) fn private_file_identity(path: &Path) -> Result<Option<FileIdentity>, StoreError> {
    require_private_regular_file(path)?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| StoreError::io("inspect private backup file identity", source))?;
    Ok(FileIdentity::from_metadata(&metadata))
}

pub(super) fn sync_file(path: &Path) -> Result<(), StoreError> {
    let file = open_readonly_nofollow(path)?;
    file.sync_all()
        .map_err(|source| StoreError::io("sync backup artifact", source))
}

pub(super) fn sync_directory(path: &Path) -> Result<(), StoreError> {
    #[expect(
        clippy::disallowed_methods,
        reason = "directory durability sync runs inside manifest::run_blocking"
    )]
    let file =
        File::open(path).map_err(|source| StoreError::io("open backup directory", source))?;
    file.sync_all()
        .map_err(|source| StoreError::io("sync backup directory", source))
}

pub(super) fn open_readonly_nofollow(path: &Path) -> Result<CustodiedFile, StoreError> {
    open_readonly_nofollow_expected(path, None)
}

pub(super) fn open_readonly_nofollow_expected(
    path: &Path,
    expected_identity: Option<FileIdentity>,
) -> Result<CustodiedFile, StoreError> {
    require_private_regular_file(path)?;
    let before_path = fs::symlink_metadata(path)
        .map_err(|source| StoreError::io("inspect backup artifact before open", source))?;
    let before_identity = FileIdentity::from_metadata(&before_path);
    if expected_identity.is_some() && expected_identity != before_identity {
        return Err(StoreError::InvalidManifest);
    }

    let lease = super::registry::reserve_artifact_identity(before_identity)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::fcntl::OFlag::O_NOFOLLOW.bits());
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "backup artifact reads run inside manifest::run_blocking"
    )]
    let file = options
        .open(path)
        .map_err(|source| StoreError::io("open backup artifact", source))?;
    let opened_metadata = file
        .metadata()
        .map_err(|source| StoreError::io("inspect opened backup artifact", source))?;
    let current_path = fs::symlink_metadata(path)
        .map_err(|source| StoreError::io("recheck backup artifact path", source))?;
    let opened_identity = FileIdentity::from_metadata(&opened_metadata);

    if !opened_metadata.is_file()
        || FileIdentity::is_multiply_linked(&opened_metadata)
        || opened_identity != before_identity
        || FileIdentity::from_metadata(&current_path) != before_identity
        || (expected_identity.is_some() && opened_identity != expected_identity)
    {
        return Err(StoreError::InvalidManifest);
    }
    Ok(CustodiedFile {
        file,
        _lease: lease,
    })
}

pub(super) struct CustodiedFile {
    file: File,
    _lease: super::registry::IdentityLease,
}

impl Deref for CustodiedFile {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        &self.file
    }
}

impl DerefMut for CustodiedFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.file
    }
}

pub(super) fn read_manifest(path: &Path) -> Result<BTreeMap<String, String>, StoreError> {
    read_manifest_expected(path, None)
}

pub(super) fn read_manifest_expected(
    path: &Path,
    expected_identity: Option<FileIdentity>,
) -> Result<BTreeMap<String, String>, StoreError> {
    let mut file = open_readonly_nofollow_expected(path, expected_identity)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|source| StoreError::io("read backup manifest", source))?;
    let mut fields = BTreeMap::new();
    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            return Err(StoreError::InvalidManifest);
        };
        if key.is_empty() || fields.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(StoreError::InvalidManifest);
        }
    }
    Ok(fields)
}

pub(super) fn sha256_file(path: &Path) -> Result<(String, u64), StoreError> {
    sha256_file_expected(path, None)
}

pub(super) fn sha256_file_expected(
    path: &Path,
    expected_identity: Option<FileIdentity>,
) -> Result<(String, u64), StoreError> {
    let mut file = open_readonly_nofollow_expected(path, expected_identity)?;
    let metadata = file
        .metadata()
        .map_err(|source| StoreError::io("inspect backup artifact", source))?;
    if !metadata.is_file() {
        return Err(StoreError::BackupIncomplete);
    }
    let length = metadata.len();
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| StoreError::io("hash backup artifact", source))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok((hex::encode(hasher.finalize()), length))
}

pub(super) fn manifest_bytes(fields: &[(String, String)]) -> Vec<u8> {
    let mut text = String::new();
    for (key, value) in fields {
        text.push_str(key);
        text.push('=');
        text.push_str(value);
        text.push('\n');
    }
    text.into_bytes()
}
