mod backup;
mod forensics;
mod logical;
mod manifest;
mod path;
mod registry;

use std::fmt;
use std::fs::OpenOptions;
use std::io;
use std::path::Path;

use turso::{Connection, Database};

use crate::{DbOperation, operation};

pub use registry::{StoreOperationGuard, acquire_store_operation};

pub use backup::{BackupReceipt, CommittedMigrationValidation, MigrationReceipt};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ManagedStoreKind {
    QuotaSnapshot,
    CliUsage,
}

impl ManagedStoreKind {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::QuotaSnapshot => "quota_snapshot",
            Self::CliUsage => "cli_usage",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreopenStatus {
    NotCaptured,

    CapturedAbsent,

    CapturedExisting,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogicalDigest {
    pub schema_sha256: String,
    pub data_sha256: String,
    pub metadata_sha256: String,
    pub schema_objects: u64,
    pub tables: u64,
    pub rows: u64,
}

#[derive(Debug)]
pub enum StoreError {
    Io(&'static str, io::ErrorKind),
    Backend(&'static str),
    FileLockDisabled,
    UnsupportedSourcePath,
    UnsafeSourceFile,
    HardLinkedSourceFile,
    SourceIdentityChanged,
    SourceIdentityAlreadyOpened,
    SourceChangedDuringCapture,
    RegistryState,
    MigrationRequiresProcessRestart,
    ActiveMigrationTransactionRequired,
    CommitStillActive,
    StoreKindMismatch,
    BackupCollision,
    BackupIncomplete,
    LogicalBackupMismatch,
    UnsupportedSchemaObject,
    UnsupportedTableShape,
    UnsupportedDatabaseMetadata,
    CountOverflow,
    InvalidManifest,
    InvalidMigrationVersion,
    ReceiptMismatch,
    CommittedSchemaMismatch,
    ClockUnavailable,
    GenerationUnavailable,
}

impl StoreError {
    pub(super) fn io(action: &'static str, source: io::Error) -> Self {
        Self::Io(action, source.kind())
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(action, kind) => write!(f, "{action} failed ({kind:?})"),
            Self::Backend(message) => f.write_str(message),
            Self::FileLockDisabled => {
                f.write_str("local managed store requires Turso's default file lock")
            }
            Self::UnsupportedSourcePath | Self::UnsafeSourceFile | Self::HardLinkedSourceFile => {
                fmt_source_error(self, f)
            }
            Self::SourceIdentityChanged
            | Self::SourceIdentityAlreadyOpened
            | Self::SourceChangedDuringCapture
            | Self::RegistryState => fmt_capture_error(self, f),
            Self::MigrationRequiresProcessRestart
            | Self::ActiveMigrationTransactionRequired
            | Self::CommitStillActive
            | Self::StoreKindMismatch => fmt_migration_error(self, f),
            Self::BackupCollision | Self::BackupIncomplete | Self::LogicalBackupMismatch => {
                fmt_backup_error(self, f)
            }
            Self::UnsupportedSchemaObject
            | Self::UnsupportedTableShape
            | Self::UnsupportedDatabaseMetadata
            | Self::CountOverflow => fmt_schema_error(self, f),
            Self::InvalidManifest
            | Self::InvalidMigrationVersion
            | Self::ReceiptMismatch
            | Self::CommittedSchemaMismatch
            | Self::ClockUnavailable
            | Self::GenerationUnavailable => fmt_receipt_error(self, f),
        }
    }
}

fn fmt_source_error(error: &StoreError, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match error {
        StoreError::UnsupportedSourcePath => f.write_str("managed store path is unsupported"),
        StoreError::UnsafeSourceFile => f.write_str("managed store source is not a regular file"),
        StoreError::HardLinkedSourceFile => {
            f.write_str("managed store source has multiple hard links")
        }
        _ => Err(fmt::Error),
    }
}

fn fmt_capture_error(error: &StoreError, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match error {
        StoreError::SourceIdentityChanged => f.write_str("managed store source identity changed"),
        StoreError::SourceIdentityAlreadyOpened => {
            f.write_str("managed store source identity was already opened through an alias")
        }
        StoreError::SourceChangedDuringCapture => {
            f.write_str("source changed during forensic capture")
        }
        StoreError::RegistryState => f.write_str("managed store registry state is inconsistent"),
        _ => Err(fmt::Error),
    }
}

fn fmt_migration_error(error: &StoreError, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match error {
        StoreError::MigrationRequiresProcessRestart => {
            f.write_str("old managed store requires a process restart before migration")
        }
        StoreError::ActiveMigrationTransactionRequired => {
            f.write_str("migration backup requires an active mutation-free transaction")
        }
        StoreError::CommitStillActive => f.write_str("migration commit is not complete"),
        StoreError::StoreKindMismatch => f.write_str("managed store kind does not match its owner"),
        _ => Err(fmt::Error),
    }
}

fn fmt_backup_error(error: &StoreError, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match error {
        StoreError::BackupCollision => f.write_str("backup destination already exists"),
        StoreError::BackupIncomplete => f.write_str("logical backup is incomplete"),
        StoreError::LogicalBackupMismatch => {
            f.write_str("logical backup does not match its source")
        }
        _ => Err(fmt::Error),
    }
}

fn fmt_schema_error(error: &StoreError, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match error {
        StoreError::UnsupportedSchemaObject => {
            f.write_str("database contains an unsupported schema object")
        }
        StoreError::UnsupportedTableShape => {
            f.write_str("database contains an unsupported table shape")
        }
        StoreError::UnsupportedDatabaseMetadata => {
            f.write_str("database metadata cannot be validated")
        }
        StoreError::CountOverflow => f.write_str("logical database count overflowed"),
        _ => Err(fmt::Error),
    }
}

fn fmt_receipt_error(error: &StoreError, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match error {
        StoreError::InvalidManifest => f.write_str("backup manifest is invalid"),
        StoreError::InvalidMigrationVersion => f.write_str("migration versions are invalid"),
        StoreError::ReceiptMismatch => {
            f.write_str("backup receipt does not match this managed store")
        }
        StoreError::CommittedSchemaMismatch => {
            f.write_str("committed store schema does not match its migration receipt")
        }
        StoreError::ClockUnavailable => f.write_str("system clock is unavailable"),
        StoreError::GenerationUnavailable => {
            f.write_str("could not allocate a unique backup generation")
        }
        _ => Err(fmt::Error),
    }
}

impl std::error::Error for StoreError {}

pub struct LocalStore {
    database: Database,
    primary: Connection,
    resolved: path::ResolvedPath,
    kind: ManagedStoreKind,
    evidence: Option<forensics::PreopenEvidence>,
    path_hash: String,
}

pub(crate) async fn build_legacy_database(path: &str) -> Result<Database, String> {
    let resolved = path::resolve_legacy_path(Path::new(path))
        .map_err(|error| format!("track legacy local store open failed: {error}"))?;
    let permit = registry::begin_legacy_builder(&resolved)
        .await
        .map_err(|error| format!("track legacy local store open failed: {error}"))?;
    let database = turso::Builder::new_local(path).build().await;
    let identity = path::resolve_legacy_path(&resolved.path)
        .map_err(|error| format!("track completed legacy local store open failed: {error}"))?
        .identity;
    registry::finish_legacy_builder(permit, identity);
    database.map_err(|_| "open local store failed".to_owned())
}

impl fmt::Debug for LocalStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalStore")
            .field("kind", &self.kind)
            .field("preopen_status", &self.preopen_status())
            .finish_non_exhaustive()
    }
}

impl LocalStore {
    pub async fn open_managed(
        path: &Path,
        kind: ManagedStoreKind,
        operation_guard: &StoreOperationGuard,
    ) -> Result<Self, StoreError> {
        if std::env::var_os("LIMBO_DISABLE_FILE_LOCK").is_some() {
            return Err(StoreError::FileLockDisabled);
        }
        let resolved = operation_guard.validate_path(path)?;
        path::ensure_managed_sql_path(&resolved.path)?;
        let path_text = resolved
            .path
            .to_str()
            .ok_or(StoreError::UnsupportedSourcePath)?;
        let permit = registry::reserve_managed_open(&resolved).await?;
        let prepare_path = resolved.clone();
        let (permit, evidence, expected_identity) =
            manifest::run_blocking("managed open preparation task failed", move || {
                prepare_managed_open(prepare_path, permit)
            })
            .await?;

        let database_result = operation(
            DbOperation::Connect,
            turso::Builder::new_local(path_text).build(),
        )
        .await;
        let after_build = path::resolve_source_path(&resolved.path);
        let after_identity = after_build.as_ref().ok().and_then(|path| path.identity);
        registry::finish_managed_builder(permit, after_identity);
        let database =
            database_result.map_err(|_| StoreError::Backend("open managed store failed"))?;
        let after_build = after_build?;
        if after_build.path != resolved.path || after_build.identity != expected_identity {
            return Err(StoreError::SourceIdentityChanged);
        }
        let primary = database
            .connect()
            .map_err(|_| StoreError::Backend("connect managed store failed"))?;
        Ok(Self {
            database,
            primary,
            resolved: after_build,
            kind,
            evidence,
            path_hash: path::path_digest(&resolved.path),
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.primary
    }

    pub fn backup_connection(&self) -> Result<Connection, StoreError> {
        self.database
            .connect()
            .map_err(|_| StoreError::Backend("connect backup store failed"))
    }

    pub fn preopen_status(&self) -> PreopenStatus {
        match self.evidence.as_ref() {
            Some(evidence) if evidence.source_existed => PreopenStatus::CapturedExisting,
            Some(_) => PreopenStatus::CapturedAbsent,
            None => PreopenStatus::NotCaptured,
        }
    }

    pub async fn backup_before_migration(
        &self,
        kind: ManagedStoreKind,
        expected_old_version: Option<u64>,
        new_version: u64,
    ) -> Result<Option<BackupReceipt>, StoreError> {
        backup::backup_before_migration(self, kind, expected_old_version, new_version).await
    }

    pub async fn validate_committed_migration(
        &self,
        kind: ManagedStoreKind,
        new_version: u64,
    ) -> Result<CommittedMigrationValidation, StoreError> {
        backup::validate_committed_migration(self, kind, new_version).await
    }

    pub async fn mark_migration_committed(
        &self,
        receipt: &BackupReceipt,
        validation: CommittedMigrationValidation,
    ) -> Result<MigrationReceipt, StoreError> {
        backup::mark_migration_committed(self, receipt, validation).await
    }

    pub async fn mark_current_schema_migration_committed(
        &self,
        kind: ManagedStoreKind,
        new_version: u64,
        validation: CommittedMigrationValidation,
    ) -> Result<Option<MigrationReceipt>, StoreError> {
        backup::mark_current_schema_migration_committed(self, kind, new_version, validation).await
    }
}

fn prepare_managed_open(
    prepare_path: path::ResolvedPath,
    mut permit: registry::ManagedOpenPermit,
) -> Result<
    (
        registry::ManagedOpenPermit,
        Option<forensics::PreopenEvidence>,
        Option<path::FileIdentity>,
    ),
    StoreError,
> {
    if !permit.preopen_reserved {
        let evidence = permit.evidence.clone();
        return Ok((permit, evidence, prepare_path.identity));
    }

    let (evidence, source_identities, identity_lease) =
        forensics::capture_preopen(&prepare_path, &permit)?;
    let mut opened_path = path::resolve_source_path(&prepare_path.path)?;
    if let Err(error) = path::ensure_same_source(&prepare_path, &opened_path)
        && (prepare_path.identity.is_some() || opened_path.identity.is_some())
    {
        return Err(error);
    }
    if !evidence.source_existed {
        create_empty_source(&prepare_path.path)?;
        opened_path = path::resolve_source_path(&prepare_path.path)?;
    }
    if !evidence.source_existed && opened_path.identity.is_none() {
        return Err(StoreError::SourceIdentityChanged);
    }
    registry::complete_preopen_and_begin_builder(
        &mut permit,
        evidence.clone(),
        opened_path.identity,
        source_identities,
        identity_lease,
    )?;
    #[cfg(test)]
    registry::block_managed_prepare_for_test(&prepare_path.path);

    Ok((permit, Some(evidence), opened_path.identity))
}

fn create_empty_source(path: &Path) -> Result<(), StoreError> {
    let mut options = OpenOptions::new();
    options.write(true).read(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o666);
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "fresh source file creation runs inside manifest::run_blocking"
    )]
    let file = options
        .open(path)
        .map_err(|source| StoreError::io("create managed store source", source))?;
    file.sync_all()
        .map_err(|source| StoreError::io("sync managed store source", source))?;
    drop(file);
    manifest::sync_directory(path.parent().ok_or(StoreError::UnsupportedSourcePath)?)
}

#[cfg(test)]
mod tests;
