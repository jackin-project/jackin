use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use turso::{Connection, Value};

use super::forensics::PreopenEvidence;
use super::manifest::{
    create_generation_dir_on_worker, manifest_bytes, read_manifest, read_manifest_on_worker,
    run_blocking, sync_file_on_worker, write_immutable_on_worker,
};
use super::path::{FileIdentity, identity_fields, resolve_source_path};
use super::{LocalStore, LogicalDigest, ManagedStoreKind, StoreError};

const READY_FORMAT: &str = "jackin-quota-logical-backup-v1";
const READY_NAME: &str = "backup_ready.manifest";
const BACKUP_NAME: &str = "rollback.sqlite";
const MIGRATED_NAME: &str = "migration_committed.manifest";

pub struct BackupReceipt {
    pub(super) directory: PathBuf,
    path_hash: String,
    source_identity: Option<FileIdentity>,
    kind: ManagedStoreKind,
    old_version: Option<u64>,
    new_version: u64,
    generation: String,
    backup_identity: Option<FileIdentity>,
    backup_sha256: String,
    backup_length: u64,
    ready_manifest_sha256: String,
    logical_digest: LogicalDigest,
}

impl BackupReceipt {
    pub fn kind(&self) -> ManagedStoreKind {
        self.kind
    }

    pub fn old_version(&self) -> Option<u64> {
        self.old_version
    }

    pub fn new_version(&self) -> u64 {
        self.new_version
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }

    pub fn logical_digest(&self) -> &LogicalDigest {
        &self.logical_digest
    }
}

impl fmt::Debug for BackupReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BackupReceipt")
            .field("kind", &self.kind)
            .field("old_version", &self.old_version)
            .field("new_version", &self.new_version)
            .field("generation", &self.generation)
            .field("logical_digest", &self.logical_digest)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationReceipt {
    kind: ManagedStoreKind,
    new_version: u64,
    generation: String,
}

pub struct CommittedMigrationValidation {
    connection: Connection,
    path_hash: String,
    source_identity: Option<FileIdentity>,
    kind: ManagedStoreKind,
    new_version: u64,
    logical_digest: LogicalDigest,
}

impl CommittedMigrationValidation {
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

impl fmt::Debug for CommittedMigrationValidation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommittedMigrationValidation")
            .field("kind", &self.kind)
            .field("new_version", &self.new_version)
            .field("logical_digest", &self.logical_digest)
            .finish_non_exhaustive()
    }
}

impl MigrationReceipt {
    pub fn kind(&self) -> ManagedStoreKind {
        self.kind
    }

    pub fn new_version(&self) -> u64 {
        self.new_version
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }
}

pub(super) async fn backup_before_migration(
    store: &LocalStore,
    kind: ManagedStoreKind,
    expected_old_version: Option<u64>,
    new_version: u64,
) -> Result<Option<BackupReceipt>, StoreError> {
    let Some(evidence) =
        validate_backup_preconditions(store, kind, expected_old_version, new_version).await?
    else {
        return Ok(None);
    };

    let (directory, generation) =
        create_generation_dir_on_worker(&store.resolved.parent, &store.path_hash).await?;
    let backup_path = directory.join(BACKUP_NAME);
    write_incomplete_backup_manifest(
        &directory,
        store,
        kind,
        expected_old_version,
        new_version,
        &generation,
    )
    .await?;

    let backup_connection = store.backup_connection()?;
    let logical_digest = super::logical::create_and_validate_backup(
        &store.primary,
        &backup_connection,
        &backup_path,
    )
    .await?;
    drop(backup_connection);
    let backup_identity = super::manifest::private_file_identity(&backup_path)?;
    if store
        .primary
        .is_autocommit()
        .map_err(|_| StoreError::Backend("inspect migration transaction failed"))?
    {
        return Err(StoreError::ActiveMigrationTransactionRequired);
    }
    sync_file_on_worker(&backup_path).await?;
    let (backup_sha256, backup_length) =
        super::manifest::sha256_file_on_worker_expected(&backup_path, backup_identity).await?;
    let fields = ready_manifest_fields(
        store,
        evidence,
        ReadyManifestData {
            kind,
            old_version: expected_old_version,
            new_version,
            generation: &generation,
            backup_identity,
            backup_length,
            backup_sha256: &backup_sha256,
            logical_digest: &logical_digest,
        },
    )?;
    let ready_manifest = manifest_bytes(&fields);
    let ready_manifest_sha256 = hex::encode(Sha256::digest(&ready_manifest));
    write_immutable_on_worker(&directory, READY_NAME, ready_manifest).await?;
    Ok(Some(BackupReceipt {
        directory,
        path_hash: store.path_hash.clone(),
        source_identity: store.resolved.identity,
        kind,
        old_version: expected_old_version,
        new_version,
        generation,
        backup_identity,
        backup_sha256,
        backup_length,
        ready_manifest_sha256,
        logical_digest,
    }))
}

async fn validate_backup_preconditions(
    store: &LocalStore,
    kind: ManagedStoreKind,
    expected_old_version: Option<u64>,
    new_version: u64,
) -> Result<Option<&PreopenEvidence>, StoreError> {
    if kind != store.kind {
        return Err(StoreError::StoreKindMismatch);
    }
    validate_versions(expected_old_version, new_version)?;
    if store
        .primary
        .is_autocommit()
        .map_err(|_| StoreError::Backend("inspect migration transaction failed"))?
    {
        return Err(StoreError::ActiveMigrationTransactionRequired);
    }
    ensure_source_identity(store)?;

    let evidence = store
        .evidence
        .as_ref()
        .ok_or(StoreError::MigrationRequiresProcessRestart)?;
    validate_preopen_evidence(store, evidence).await?;
    if !evidence.source_existed {
        if expected_old_version.is_none()
            && super::logical::is_fresh_empty_database(&store.primary).await?
        {
            return Ok(None);
        }
    } else if evidence.source_identity != store.resolved.identity {
        return Err(StoreError::SourceIdentityChanged);
    }
    Ok(Some(evidence))
}

async fn write_incomplete_backup_manifest(
    directory: &Path,
    store: &LocalStore,
    kind: ManagedStoreKind,
    old_version: Option<u64>,
    new_version: u64,
    generation: &str,
) -> Result<(), StoreError> {
    let incomplete = manifest_bytes(&[
        ("format".to_owned(), READY_FORMAT.to_owned()),
        ("status".to_owned(), "backup_incomplete".to_owned()),
        ("generation".to_owned(), generation.to_owned()),
        ("source_path_sha256".to_owned(), store.path_hash.clone()),
        ("kind".to_owned(), kind.as_str().to_owned()),
        ("old_version".to_owned(), version_text(old_version)),
        ("new_version".to_owned(), new_version.to_string()),
    ]);
    write_immutable_on_worker(directory, "backup_incomplete.manifest", incomplete).await?;
    Ok(())
}

struct ReadyManifestData<'a> {
    kind: ManagedStoreKind,
    old_version: Option<u64>,
    new_version: u64,
    generation: &'a str,
    backup_identity: Option<FileIdentity>,
    backup_length: u64,
    backup_sha256: &'a str,
    logical_digest: &'a LogicalDigest,
}

fn ready_manifest_fields(
    store: &LocalStore,
    evidence: &PreopenEvidence,
    data: ReadyManifestData<'_>,
) -> Result<Vec<(String, String)>, StoreError> {
    let created_unix_ns = now_unix_ns()?;
    let (source_device, source_inode) = identity_fields(store.resolved.identity);
    let (preopen_device, preopen_inode) = identity_fields(evidence.source_identity);
    let (backup_device, backup_inode) = identity_fields(data.backup_identity);
    let (forensic_manifest_device, forensic_manifest_inode) =
        identity_fields(evidence.manifest_identity);
    let mut fields = vec![
        ("format".to_owned(), READY_FORMAT.to_owned()),
        ("status".to_owned(), "backup_ready".to_owned()),
        ("generation".to_owned(), data.generation.to_owned()),
        ("source_path_sha256".to_owned(), store.path_hash.clone()),
        ("source_device".to_owned(), source_device),
        ("source_inode".to_owned(), source_inode),
        ("kind".to_owned(), data.kind.as_str().to_owned()),
        ("old_version".to_owned(), version_text(data.old_version)),
        ("new_version".to_owned(), data.new_version.to_string()),
        ("backup_file".to_owned(), BACKUP_NAME.to_owned()),
        ("backup_artifact_device".to_owned(), backup_device),
        ("backup_artifact_inode".to_owned(), backup_inode),
        ("backup_length".to_owned(), data.backup_length.to_string()),
        ("backup_sha256".to_owned(), data.backup_sha256.to_owned()),
        (
            "schema_sha256".to_owned(),
            data.logical_digest.schema_sha256.clone(),
        ),
        (
            "data_sha256".to_owned(),
            data.logical_digest.data_sha256.clone(),
        ),
        (
            "metadata_sha256".to_owned(),
            data.logical_digest.metadata_sha256.clone(),
        ),
        (
            "schema_objects".to_owned(),
            data.logical_digest.schema_objects.to_string(),
        ),
        ("tables".to_owned(), data.logical_digest.tables.to_string()),
        ("rows".to_owned(), data.logical_digest.rows.to_string()),
        (
            "forensic_status".to_owned(),
            "preopen_noncoherent".to_owned(),
        ),
        (
            "forensic_generation".to_owned(),
            evidence.generation.clone(),
        ),
        (
            "forensic_manifest_sha256".to_owned(),
            evidence.manifest_sha256.clone(),
        ),
        (
            "forensic_manifest_device".to_owned(),
            forensic_manifest_device,
        ),
        (
            "forensic_manifest_inode".to_owned(),
            forensic_manifest_inode,
        ),
        ("forensic_source_device".to_owned(), preopen_device),
        ("forensic_source_inode".to_owned(), preopen_inode),
        ("created_unix_ns".to_owned(), created_unix_ns),
    ];
    fields.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(fields)
}

pub(super) async fn validate_committed_migration(
    store: &LocalStore,
    kind: ManagedStoreKind,
    new_version: u64,
) -> Result<CommittedMigrationValidation, StoreError> {
    if kind != store.kind {
        return Err(StoreError::StoreKindMismatch);
    }
    if new_version == 0 {
        return Err(StoreError::InvalidMigrationVersion);
    }
    if !store
        .primary
        .is_autocommit()
        .map_err(|_| StoreError::Backend("inspect committed migration transaction failed"))?
    {
        return Err(StoreError::CommitStillActive);
    }
    ensure_source_identity(store)?;
    let connection = store
        .database
        .connect()
        .map_err(|_| StoreError::Backend("connect committed validation store failed"))?;
    if !connection
        .is_autocommit()
        .map_err(|_| StoreError::Backend("inspect validation transaction failed"))?
    {
        return Err(StoreError::CommitStillActive);
    }
    validate_schema_version(&connection, new_version).await?;
    let logical_digest = super::logical::logical_digest(&connection).await?;
    validate_schema_version(&connection, new_version).await?;
    if !connection
        .is_autocommit()
        .map_err(|_| StoreError::Backend("inspect validation transaction failed"))?
    {
        return Err(StoreError::CommitStillActive);
    }
    ensure_source_identity(store)?;
    Ok(CommittedMigrationValidation {
        connection,
        path_hash: store.path_hash.clone(),
        source_identity: store.resolved.identity,
        kind,
        new_version,
        logical_digest,
    })
}

pub(super) async fn mark_migration_committed(
    store: &LocalStore,
    receipt: &BackupReceipt,
    validation: CommittedMigrationValidation,
) -> Result<MigrationReceipt, StoreError> {
    validate_validation_token(store, &validation, receipt.kind, receipt.new_version).await?;
    if !store
        .primary
        .is_autocommit()
        .map_err(|_| StoreError::Backend("inspect migration transaction failed"))?
    {
        return Err(StoreError::CommitStillActive);
    }
    ensure_source_identity(store)?;
    if receipt.kind != store.kind
        || receipt.path_hash != store.path_hash
        || receipt.source_identity != store.resolved.identity
    {
        return Err(StoreError::ReceiptMismatch);
    }
    let ready = validate_ready_directory(
        &receipt.directory,
        &store.resolved.parent,
        &store.path_hash,
        store.resolved.identity,
        receipt.kind,
        receipt.new_version,
    )
    .await?;
    if ready.generation != receipt.generation
        || ready.backup_identity != receipt.backup_identity
        || ready.backup_sha256 != receipt.backup_sha256
        || ready.backup_length != receipt.backup_length
        || ready.logical_digest != receipt.logical_digest
        || ready.manifest_sha256 != receipt.ready_manifest_sha256
        || ready.old_version != receipt.old_version
    {
        return Err(StoreError::ReceiptMismatch);
    }
    publish_migrated(&receipt.directory, &ready.fields).await?;
    Ok(MigrationReceipt {
        kind: receipt.kind,
        new_version: receipt.new_version,
        generation: receipt.generation.clone(),
    })
}

pub(super) async fn mark_current_schema_migration_committed(
    store: &LocalStore,
    kind: ManagedStoreKind,
    new_version: u64,
    validation: CommittedMigrationValidation,
) -> Result<Option<MigrationReceipt>, StoreError> {
    validate_validation_token(store, &validation, kind, new_version).await?;
    if kind != store.kind {
        return Err(StoreError::StoreKindMismatch);
    }
    if new_version == 0 {
        return Err(StoreError::InvalidMigrationVersion);
    }
    if !store
        .primary
        .is_autocommit()
        .map_err(|_| StoreError::Backend("inspect migration transaction failed"))?
    {
        return Err(StoreError::CommitStillActive);
    }
    ensure_source_identity(store)?;

    let parent = store.resolved.parent.clone();
    let path_hash = store.path_hash.clone();
    let prefix = backup_directory_prefix(&store.path_hash);
    let identity = store.resolved.identity;
    let kind_text = kind.as_str().to_owned();
    let latest = run_blocking("scan backup generations task failed", move || {
        latest_ready_generation(
            &parent,
            &prefix,
            &path_hash,
            identity,
            &kind_text,
            new_version,
        )
    })
    .await?;
    let Some((_, generation, directory)) = latest else {
        return Ok(None);
    };
    let ready = validate_ready_directory(
        &directory,
        &store.resolved.parent,
        &store.path_hash,
        store.resolved.identity,
        kind,
        new_version,
    )
    .await?;
    if ready.generation != generation {
        return Err(StoreError::InvalidManifest);
    }
    if has_valid_migrated_marker(&directory, &ready.fields).await? {
        return Ok(None);
    }
    publish_migrated(&directory, &ready.fields).await?;
    Ok(Some(MigrationReceipt {
        kind,
        new_version,
        generation: ready.generation,
    }))
}

async fn validate_validation_token(
    store: &LocalStore,
    validation: &CommittedMigrationValidation,
    kind: ManagedStoreKind,
    new_version: u64,
) -> Result<(), StoreError> {
    if kind != store.kind
        || validation.kind != kind
        || validation.new_version != new_version
        || validation.path_hash != store.path_hash
        || validation.source_identity != store.resolved.identity
    {
        return Err(StoreError::ReceiptMismatch);
    }
    if !store
        .primary
        .is_autocommit()
        .map_err(|_| StoreError::Backend("inspect committed migration transaction failed"))?
        || !validation
            .connection
            .is_autocommit()
            .map_err(|_| StoreError::Backend("inspect validation transaction failed"))?
    {
        return Err(StoreError::CommitStillActive);
    }
    ensure_source_identity(store)?;
    validate_schema_version(&validation.connection, new_version).await?;
    let digest = super::logical::logical_digest(&validation.connection).await?;
    if digest != validation.logical_digest {
        return Err(StoreError::CommittedSchemaMismatch);
    }
    ensure_source_identity(store)?;
    Ok(())
}

async fn validate_schema_version(
    connection: &Connection,
    expected_version: u64,
) -> Result<(), StoreError> {
    let mut rows = connection
        .query("SELECT value FROM _meta WHERE key = 'schema_version'", ())
        .await
        .map_err(|_| StoreError::CommittedSchemaMismatch)?;
    let row = rows
        .next()
        .await
        .map_err(|_| StoreError::CommittedSchemaMismatch)?
        .ok_or(StoreError::CommittedSchemaMismatch)?;
    let Value::Text(version) = row
        .get_value(0)
        .map_err(|_| StoreError::CommittedSchemaMismatch)?
    else {
        return Err(StoreError::CommittedSchemaMismatch);
    };
    if version != expected_version.to_string()
        || rows
            .next()
            .await
            .map_err(|_| StoreError::CommittedSchemaMismatch)?
            .is_some()
    {
        return Err(StoreError::CommittedSchemaMismatch);
    }
    Ok(())
}

fn latest_ready_generation(
    parent: &Path,
    prefix: &str,
    path_hash: &str,
    identity: Option<FileIdentity>,
    kind: &str,
    new_version: u64,
) -> Result<Option<(u128, String, PathBuf)>, StoreError> {
    let entries =
        fs::read_dir(parent).map_err(|source| StoreError::io("list backup generations", source))?;
    let mut latest: Option<(u128, String, PathBuf)> = None;
    for entry in entries {
        let entry =
            entry.map_err(|source| StoreError::io("read backup generation entry", source))?;
        let Some(candidate) =
            ready_generation_candidate(entry, prefix, path_hash, identity, kind, new_version)?
        else {
            continue;
        };
        if is_later_generation(&candidate, latest.as_ref()) {
            latest = Some(candidate);
        }
    }
    Ok(latest)
}

fn ready_generation_candidate(
    entry: fs::DirEntry,
    prefix: &str,
    path_hash: &str,
    identity: Option<FileIdentity>,
    kind: &str,
    new_version: u64,
) -> Result<Option<(u128, String, PathBuf)>, StoreError> {
    if !entry.file_name().to_string_lossy().starts_with(prefix) {
        return Ok(None);
    }
    let directory = entry.path();
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        _ => return Ok(None),
    }
    let ready_path = directory.join(READY_NAME);
    let fields = match read_manifest(&ready_path) {
        Ok(fields) => fields,
        Err(StoreError::Io(_, std::io::ErrorKind::NotFound)) => return Ok(None),
        Err(error) => return Err(error),
    };
    if field(&fields, "format") != Some(READY_FORMAT)
        || field(&fields, "status") != Some("backup_ready")
        || field(&fields, "source_path_sha256") != Some(path_hash)
        || field(&fields, "kind") != Some(kind)
        || field(&fields, "new_version").and_then(|value| value.parse::<u64>().ok())
            != Some(new_version)
        || !identity_matches(&fields, "source", identity)
    {
        return Ok(None);
    }
    let created = required(&fields, "created_unix_ns")?
        .parse::<u128>()
        .map_err(|_| StoreError::InvalidManifest)?;
    let generation = required(&fields, "generation")?.to_owned();
    Ok(Some((created, generation, directory)))
}

fn is_later_generation(
    candidate: &(u128, String, PathBuf),
    latest: Option<&(u128, String, PathBuf)>,
) -> bool {
    latest.is_none_or(|(latest_created, latest_generation, _)| {
        candidate.0 > *latest_created
            || (candidate.0 == *latest_created && candidate.1.as_str() > latest_generation.as_str())
    })
}

struct ValidatedReady {
    fields: BTreeMap<String, String>,
    manifest_sha256: String,
    generation: String,
    old_version: Option<u64>,
    backup_sha256: String,
    backup_length: u64,
    backup_identity: Option<FileIdentity>,
    logical_digest: LogicalDigest,
}

async fn validate_ready_directory(
    directory: &Path,
    source_parent: &Path,
    path_hash: &str,
    identity: Option<FileIdentity>,
    kind: ManagedStoreKind,
    new_version: u64,
) -> Result<ValidatedReady, StoreError> {
    let (manifest_sha256, fields) = read_ready_manifest(directory).await?;
    validate_ready_bindings(&fields, path_hash, identity, kind, new_version)?;
    let (generation, backup_path, backup_identity) =
        resolve_ready_backup(directory, source_parent, path_hash, &fields)?;
    let (backup_sha256, backup_length) =
        validate_ready_backup_bytes(&backup_path, backup_identity, &fields).await?;
    let logical_digest = validate_ready_logical_image(
        &backup_path,
        backup_identity,
        &backup_sha256,
        backup_length,
        &fields,
    )
    .await?;
    validate_ready_forensic_evidence(source_parent, path_hash, &fields).await?;
    let old_version = parse_ready_old_version(&fields)?;
    Ok(ValidatedReady {
        fields,
        manifest_sha256,
        generation,
        old_version,
        backup_sha256,
        backup_length,
        backup_identity,
        logical_digest,
    })
}

async fn read_ready_manifest(
    directory: &Path,
) -> Result<(String, BTreeMap<String, String>), StoreError> {
    super::manifest::require_private_directory(directory)?;
    let manifest_path = directory.join(READY_NAME);
    super::manifest::require_private_regular_file(&manifest_path)?;
    let manifest_path_for_read = manifest_path.clone();
    run_blocking("read backup manifest task failed", move || {
        let manifest_sha256 = super::manifest::sha256_file(&manifest_path_for_read)?.0;
        let fields = read_manifest(&manifest_path_for_read)?;
        Ok((manifest_sha256, fields))
    })
    .await
}

fn validate_ready_bindings(
    fields: &BTreeMap<String, String>,
    path_hash: &str,
    identity: Option<FileIdentity>,
    kind: ManagedStoreKind,
    new_version: u64,
) -> Result<(), StoreError> {
    if field(fields, "format") != Some(READY_FORMAT)
        || field(fields, "status") != Some("backup_ready")
        || field(fields, "source_path_sha256") != Some(path_hash)
        || field(fields, "kind") != Some(kind.as_str())
        || field(fields, "new_version").and_then(|value| value.parse::<u64>().ok())
            != Some(new_version)
        || !identity_matches(fields, "source", identity)
    {
        return Err(StoreError::ReceiptMismatch);
    }
    Ok(())
}

fn resolve_ready_backup(
    directory: &Path,
    source_parent: &Path,
    path_hash: &str,
    fields: &BTreeMap<String, String>,
) -> Result<(String, PathBuf, Option<FileIdentity>), StoreError> {
    let generation = required(fields, "generation")?.to_owned();
    let expected_name = format!("{}{}", backup_directory_prefix(path_hash), generation);
    if directory.parent() != Some(source_parent)
        || directory.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str())
    {
        return Err(StoreError::InvalidManifest);
    }
    let backup_name = required(fields, "backup_file")?;
    if backup_name != BACKUP_NAME {
        return Err(StoreError::InvalidManifest);
    }
    let backup_path = directory.join(backup_name);
    let backup_identity = identity_from_fields(fields, "backup_artifact")?;
    Ok((generation, backup_path, backup_identity))
}

async fn validate_ready_backup_bytes(
    backup_path: &Path,
    backup_identity: Option<FileIdentity>,
    fields: &BTreeMap<String, String>,
) -> Result<(String, u64), StoreError> {
    let (backup_sha256, backup_length) =
        super::manifest::sha256_file_on_worker_expected(backup_path, backup_identity).await?;
    if backup_sha256 != required(fields, "backup_sha256")?
        || backup_length.to_string() != required(fields, "backup_length")?
    {
        return Err(StoreError::BackupIncomplete);
    }
    Ok((backup_sha256, backup_length))
}

async fn validate_ready_logical_image(
    backup_path: &Path,
    backup_identity: Option<FileIdentity>,
    backup_sha256: &str,
    backup_length: u64,
    fields: &BTreeMap<String, String>,
) -> Result<LogicalDigest, StoreError> {
    let logical_digest = digest_backup_file(backup_path, backup_identity).await?;
    let (verified_sha256, verified_length) =
        super::manifest::sha256_file_on_worker_expected(backup_path, backup_identity).await?;
    if verified_sha256 != backup_sha256 || verified_length != backup_length {
        return Err(StoreError::BackupIncomplete);
    }
    validate_ready_digest_fields(&logical_digest, fields)?;
    Ok(logical_digest)
}

fn validate_ready_digest_fields(
    logical_digest: &LogicalDigest,
    fields: &BTreeMap<String, String>,
) -> Result<(), StoreError> {
    if logical_digest.schema_sha256 != required(fields, "schema_sha256")?
        || logical_digest.data_sha256 != required(fields, "data_sha256")?
        || logical_digest.metadata_sha256 != required(fields, "metadata_sha256")?
        || logical_digest.schema_objects.to_string() != required(fields, "schema_objects")?
        || logical_digest.tables.to_string() != required(fields, "tables")?
        || logical_digest.rows.to_string() != required(fields, "rows")?
    {
        return Err(StoreError::LogicalBackupMismatch);
    }
    Ok(())
}

async fn validate_ready_forensic_evidence(
    source_parent: &Path,
    path_hash: &str,
    fields: &BTreeMap<String, String>,
) -> Result<(), StoreError> {
    let forensic_generation = required(fields, "forensic_generation")?;
    let forensic_directory = source_parent.join(format!(
        "{}{}",
        backup_directory_prefix(path_hash),
        forensic_generation
    ));
    let forensic_manifest_sha256 = required(fields, "forensic_manifest_sha256")?.to_owned();
    let forensic_identity = identity_from_fields(fields, "forensic_source")?;
    let forensic_manifest_identity = identity_from_fields(fields, "forensic_manifest")?;
    let forensic_path_hash = path_hash.to_owned();
    let forensic_generation = forensic_generation.to_owned();
    let expected_parent = source_parent.to_path_buf();
    let expected_forensic = ForensicExpectation {
        manifest_sha256: forensic_manifest_sha256,
        manifest_identity: forensic_manifest_identity,
        source_identity: forensic_identity,
        source_existed: None,
    };
    run_blocking("validate ready forensic artifacts task failed", move || {
        validate_forensic_directory(
            &forensic_directory,
            &expected_parent,
            &forensic_path_hash,
            &forensic_generation,
            expected_forensic,
        )
    })
    .await
}

fn parse_ready_old_version(fields: &BTreeMap<String, String>) -> Result<Option<u64>, StoreError> {
    Ok(match required(fields, "old_version")? {
        "unversioned" => None,
        value => Some(value.parse().map_err(|_| StoreError::InvalidManifest)?),
    })
}

async fn digest_backup_file(
    path: &Path,
    expected_identity: Option<FileIdentity>,
) -> Result<LogicalDigest, StoreError> {
    super::manifest::require_private_regular_file(path)?;
    if super::manifest::private_file_identity(path)? != expected_identity {
        return Err(StoreError::InvalidManifest);
    }
    let _identity_lease = super::registry::reserve_artifact_path(path)?;
    let path_text = path.to_str().ok_or(StoreError::UnsupportedSourcePath)?;
    super::path::ensure_managed_sql_path(path)?;
    let database = turso::Builder::new_local(path_text)
        .build()
        .await
        .map_err(|_| StoreError::Backend("open published backup failed"))?;
    let connection = database
        .connect()
        .map_err(|_| StoreError::Backend("connect published backup failed"))?;
    let digest = super::logical::logical_digest(&connection).await?;
    drop(connection);
    drop(database);
    for suffix in ["-wal", "-journal"] {
        let sidecar = super::path::sidecar_path(path, suffix);
        match fs::symlink_metadata(sidecar) {
            Ok(metadata) if metadata.file_type().is_file() && metadata.len() == 0 => {}
            Ok(_) => return Err(StoreError::BackupIncomplete),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(StoreError::io("inspect published backup sidecar", source)),
        }
    }
    Ok(digest)
}

async fn publish_migrated(
    directory: &Path,
    ready_fields: &BTreeMap<String, String>,
) -> Result<(), StoreError> {
    if has_valid_migrated_marker(directory, ready_fields).await? {
        return Ok(());
    }
    let mut fields = ready_fields.clone();
    fields.insert("status".to_owned(), "migration_committed".to_owned());
    fields.insert("completed_unix_ns".to_owned(), now_unix_ns()?);
    let fields = fields.into_iter().collect::<Vec<_>>();
    write_immutable_on_worker(directory, MIGRATED_NAME, manifest_bytes(&fields)).await?;
    Ok(())
}

async fn has_valid_migrated_marker(
    directory: &Path,
    ready_fields: &BTreeMap<String, String>,
) -> Result<bool, StoreError> {
    let marker_path = directory.join(MIGRATED_NAME);
    match fs::symlink_metadata(&marker_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(source) => return Err(StoreError::io("inspect migration receipt", source)),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.file_type().is_file() => {
            return Err(StoreError::InvalidManifest);
        }
        Ok(_) => {}
    }
    super::manifest::require_private_regular_file(&marker_path)?;
    let marker = read_manifest_on_worker(&marker_path).await?;
    if field(&marker, "status") != Some("migration_committed")
        || ![
            "format",
            "generation",
            "source_path_sha256",
            "source_device",
            "source_inode",
            "kind",
            "old_version",
            "new_version",
            "backup_file",
            "backup_artifact_device",
            "backup_artifact_inode",
            "backup_length",
            "backup_sha256",
            "schema_sha256",
            "data_sha256",
            "metadata_sha256",
            "schema_objects",
            "tables",
            "rows",
            "forensic_status",
            "forensic_generation",
            "forensic_manifest_sha256",
            "forensic_manifest_device",
            "forensic_manifest_inode",
            "forensic_source_device",
            "forensic_source_inode",
            "created_unix_ns",
        ]
        .into_iter()
        .all(|key| marker.get(key) == ready_fields.get(key))
    {
        return Err(StoreError::InvalidManifest);
    }
    Ok(true)
}

async fn validate_preopen_evidence(
    store: &LocalStore,
    evidence: &PreopenEvidence,
) -> Result<(), StoreError> {
    if evidence.manifest_path.parent() != Some(evidence.directory.as_path()) {
        return Err(StoreError::InvalidManifest);
    }
    let directory = evidence.directory.clone();
    let parent = store.resolved.parent.clone();
    let path_hash = store.path_hash.clone();
    let generation = evidence.generation.clone();
    let manifest_hash = evidence.manifest_sha256.clone();
    let manifest_identity = evidence.manifest_identity;
    let source_identity = evidence.source_identity;
    let source_existed = evidence.source_existed;
    let expected_forensic = ForensicExpectation {
        manifest_sha256: manifest_hash,
        manifest_identity,
        source_identity,
        source_existed: Some(source_existed),
    };
    run_blocking("validate forensic receipt task failed", move || {
        validate_forensic_directory(
            &directory,
            &parent,
            &path_hash,
            &generation,
            expected_forensic,
        )
    })
    .await
}

struct ForensicExpectation {
    manifest_sha256: String,
    manifest_identity: Option<FileIdentity>,
    source_identity: Option<FileIdentity>,
    source_existed: Option<bool>,
}

fn validate_forensic_directory(
    directory: &Path,
    expected_parent: &Path,
    path_hash: &str,
    generation: &str,
    expected: ForensicExpectation,
) -> Result<(), StoreError> {
    let expected_name = format!("{}{}", backup_directory_prefix(path_hash), generation);
    if directory.parent() != Some(expected_parent)
        || directory.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str())
    {
        return Err(StoreError::InvalidManifest);
    }
    super::manifest::require_private_directory(directory)?;
    let fields = read_forensic_manifest(directory, &expected)?;
    validate_forensic_header(&fields, path_hash, generation, &expected)?;
    validate_forensic_artifacts(directory, &fields)?;
    Ok(())
}

fn read_forensic_manifest(
    directory: &Path,
    expected: &ForensicExpectation,
) -> Result<BTreeMap<String, String>, StoreError> {
    let manifest_path = directory.join("preopen.manifest");
    super::manifest::require_private_regular_file(&manifest_path)?;
    let (manifest_sha256, _) =
        super::manifest::sha256_file_expected(&manifest_path, expected.manifest_identity)?;
    if manifest_sha256 != expected.manifest_sha256 {
        return Err(StoreError::InvalidManifest);
    }
    super::manifest::read_manifest_expected(&manifest_path, expected.manifest_identity)
}

fn validate_forensic_header(
    fields: &BTreeMap<String, String>,
    path_hash: &str,
    generation: &str,
    expected: &ForensicExpectation,
) -> Result<(), StoreError> {
    if field(fields, "format") != Some("jackin-quota-forensic-v1")
        || field(fields, "status") != Some("forensic_preopen_noncoherent")
        || field(fields, "generation") != Some(generation)
        || field(fields, "source_path_sha256") != Some(path_hash)
        || !identity_matches(fields, "source", expected.source_identity)
    {
        return Err(StoreError::InvalidManifest);
    }
    let source_existed = parse_bool(required(fields, "source_existed")?)?;
    if expected
        .source_existed
        .is_some_and(|expected| expected != source_existed)
    {
        return Err(StoreError::InvalidManifest);
    }
    let main_exists = parse_bool(required(fields, "source_main_exists")?)?;
    if main_exists != source_existed {
        return Err(StoreError::InvalidManifest);
    }
    Ok(())
}

fn validate_forensic_artifacts(
    directory: &Path,
    fields: &BTreeMap<String, String>,
) -> Result<(), StoreError> {
    let mut artifact_identities = std::collections::HashSet::new();
    for name in ["main", "wal", "journal"] {
        validate_forensic_artifact(directory, fields, name, &mut artifact_identities)?;
    }
    Ok(())
}

fn validate_forensic_artifact(
    directory: &Path,
    fields: &BTreeMap<String, String>,
    name: &str,
    artifact_identities: &mut std::collections::HashSet<FileIdentity>,
) -> Result<(), StoreError> {
    let source_prefix = format!("source_{name}");
    let exists = parse_bool(required(fields, &format!("{source_prefix}_exists"))?)?;
    let artifact = directory.join(format!("source.{name}"));
    if !exists {
        validate_absent_forensic_artifact(&artifact, fields, &source_prefix)?;
        return Ok(());
    }
    let expected_length = required(fields, &format!("{source_prefix}_length"))?
        .parse::<u64>()
        .map_err(|_| StoreError::InvalidManifest)?;
    let expected_sha256 = required(fields, &format!("{source_prefix}_sha256"))?;
    if hex::decode(expected_sha256).map_or(true, |bytes| bytes.len() != 32) {
        return Err(StoreError::InvalidManifest);
    }
    let source_identity = identity_from_fields(fields, &source_prefix)?;
    let artifact_identity = identity_from_fields(fields, &format!("artifact_{name}"))?;
    if artifact_identity.is_some_and(|identity| {
        !artifact_identities.insert(identity) || Some(identity) == source_identity
    }) {
        return Err(StoreError::InvalidManifest);
    }
    verify_forensic_artifact(
        &artifact,
        artifact_identity,
        expected_length,
        expected_sha256,
    )
}

fn validate_absent_forensic_artifact(
    artifact: &Path,
    fields: &BTreeMap<String, String>,
    source_prefix: &str,
) -> Result<(), StoreError> {
    if required(fields, &format!("{source_prefix}_length"))? != "absent"
        || required(fields, &format!("{source_prefix}_sha256"))? != "absent"
    {
        return Err(StoreError::InvalidManifest);
    }
    match fs::symlink_metadata(artifact) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(StoreError::InvalidManifest),
        Err(source) => Err(StoreError::io("inspect absent forensic artifact", source)),
    }
}

fn verify_forensic_artifact(
    artifact: &Path,
    artifact_identity: Option<FileIdentity>,
    expected_length: u64,
    expected_sha256: &str,
) -> Result<(), StoreError> {
    let (actual_identity, actual_length, actual_sha256) =
        hash_forensic_artifact(artifact, artifact_identity)?;
    if actual_identity != artifact_identity
        || actual_length != expected_length
        || actual_sha256 != expected_sha256
    {
        return Err(StoreError::InvalidManifest);
    }
    Ok(())
}

fn hash_forensic_artifact(
    path: &Path,
    expected_identity: Option<FileIdentity>,
) -> Result<(Option<FileIdentity>, u64, String), StoreError> {
    super::manifest::require_private_regular_file(path)?;
    let before_path = fs::symlink_metadata(path)
        .map_err(|source| StoreError::io("inspect forensic artifact", source))?;
    if before_path.file_type().is_symlink() || !before_path.file_type().is_file() {
        return Err(StoreError::InvalidManifest);
    }
    let mut file = super::manifest::open_readonly_nofollow_expected(path, expected_identity)?;
    let before = file
        .metadata()
        .map_err(|source| StoreError::io("inspect open forensic artifact", source))?;
    if !before.is_file() {
        return Err(StoreError::InvalidManifest);
    }
    let identity = FileIdentity::from_metadata(&before);
    let length = before.len();
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| StoreError::io("hash forensic artifact", source))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let after = file
        .metadata()
        .map_err(|source| StoreError::io("recheck open forensic artifact", source))?;
    let after_path = fs::symlink_metadata(path)
        .map_err(|source| StoreError::io("recheck forensic artifact path", source))?;
    if !after_path.file_type().is_file()
        || after_path.file_type().is_symlink()
        || FileIdentity::from_metadata(&after) != identity
        || FileIdentity::from_metadata(&after_path) != identity
        || after.len() != length
        || after_path.len() != length
    {
        return Err(StoreError::InvalidManifest);
    }
    Ok((identity, length, hex::encode(hasher.finalize())))
}

fn identity_from_fields(
    fields: &BTreeMap<String, String>,
    prefix: &str,
) -> Result<Option<FileIdentity>, StoreError> {
    let device = required(fields, &format!("{prefix}_device"))?;
    let inode = required(fields, &format!("{prefix}_inode"))?;
    #[cfg(unix)]
    {
        if device == "absent" && inode == "absent" {
            return Ok(None);
        }
        if device == "absent" || inode == "absent" {
            return Err(StoreError::InvalidManifest);
        }
        Ok(Some(FileIdentity {
            device: device.parse().map_err(|_| StoreError::InvalidManifest)?,
            inode: inode.parse().map_err(|_| StoreError::InvalidManifest)?,
        }))
    }
    #[cfg(not(unix))]
    {
        if (device == "unknown" && inode == "unknown") || (device == "absent" && inode == "absent")
        {
            Ok(None)
        } else {
            Err(StoreError::InvalidManifest)
        }
    }
}

fn parse_bool(value: &str) -> Result<bool, StoreError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(StoreError::InvalidManifest),
    }
}

fn ensure_source_identity(store: &LocalStore) -> Result<(), StoreError> {
    let current = resolve_source_path(&store.resolved.path)?;
    if current.path != store.resolved.path || current.identity != store.resolved.identity {
        return Err(StoreError::SourceIdentityChanged);
    }
    Ok(())
}

fn identity_matches(
    fields: &BTreeMap<String, String>,
    prefix: &str,
    identity: Option<FileIdentity>,
) -> bool {
    let (device, inode) = identity_fields(identity);
    field(fields, &format!("{prefix}_device")) == Some(device.as_str())
        && field(fields, &format!("{prefix}_inode")) == Some(inode.as_str())
}

fn field<'a>(fields: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    fields.get(key).map(String::as_str)
}

fn required<'a>(fields: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, StoreError> {
    field(fields, key).ok_or(StoreError::InvalidManifest)
}

fn validate_versions(old_version: Option<u64>, new_version: u64) -> Result<(), StoreError> {
    if new_version == 0 || old_version.is_some_and(|old| old >= new_version) {
        return Err(StoreError::InvalidMigrationVersion);
    }
    Ok(())
}

fn version_text(version: Option<u64>) -> String {
    version.map_or_else(|| "unversioned".to_owned(), |value| value.to_string())
}

fn now_unix_ns() -> Result<String, StoreError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos().to_string())
        .map_err(|_| StoreError::ClockUnavailable)
}

fn backup_directory_prefix(path_hash: &str) -> String {
    format!(".jackin-quota-backup-{}-", &path_hash[..16])
}
