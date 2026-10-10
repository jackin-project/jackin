// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! State store trait and atomic file stores.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::{
    ACCOUNT_STATE_SCHEMA_VERSION, AccountStateEnvelope, MAX_ACCOUNT_STATE_BYTES,
    PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION, PREVIOUS_PROJECTION_STATE_SCHEMA_VERSION,
    PROJECTION_STATE_SCHEMA_VERSION, STATE_QUARANTINE_COUNTER, STATE_TMP_COUNTER, StateStoreError,
    sanitize_envelope, state_filename, validate_capability, validate_envelope, validate_owned_mode,
};
use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCatalogEntry, UsageProjectionV1};
use nix::fcntl::{OFlag, open, openat, renameat};
use nix::sys::stat::Mode;
use nix::unistd::{UnlinkatFlags, fsync, unlinkat};
use serde::{Deserialize, Serialize};

/// Storage port used by the coordinator.
pub trait AccountStateStore: Send + Sync {
    /// Read and validate one account envelope.
    fn load(
        &self,
        capability: &UsageAccountCapability,
        now_epoch: i64,
    ) -> Result<Option<AccountStateEnvelope>, StateStoreError>;

    /// Atomically replace one account envelope.
    fn store(&self, envelope: &AccountStateEnvelope, now_epoch: i64)
    -> Result<(), StateStoreError>;

    /// Remove durable state for one revoked capability.
    fn purge(&self, _capability: &UsageAccountCapability) -> Result<(), StateStoreError> {
        Ok(())
    }

    /// Move unreadable durable state out of the active namespace.
    ///
    /// A catalog rotation must be able to remove a revoked account even when
    /// its old state cannot be decoded. Implementations may use a destructive
    /// purge when no recoverable quarantine namespace exists.
    fn quarantine(&self, capability: &UsageAccountCapability) -> Result<(), StateStoreError> {
        self.purge(capability)
    }
}

/// One atomic publication envelope for projection and broker metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionStateEnvelope {
    /// Persisted schema version.
    pub schema_version: u32,
    /// Immutable canonical publication.
    pub projection: UsageProjectionV1,
    /// Secret-free alias mappings committed with the publication.
    pub aliases: Vec<ProjectionAlias>,
    /// Current discovery catalog revision.
    pub catalog_revision: String,
    /// Exact capability catalog committed with the publication.
    pub catalog: Vec<UsageCatalogEntry>,
    /// Broker-owned retry deadline.
    pub retry_deadline_epoch: Option<i64>,
    /// Broker-owned success/cadence deadline.
    pub success_deadline_epoch: Option<i64>,
    /// Process incarnation that published this envelope.
    pub broker_instance_id: String,
}

impl ProjectionStateEnvelope {
    /// Current durable projection envelope schema.
    pub(crate) const SCHEMA_VERSION: u32 = PROJECTION_STATE_SCHEMA_VERSION;

    /// Previous schema containing raw host surface IDs in serialized provider
    /// rows. Only the broker startup migration path may read it.
    pub(crate) const MIGRATABLE_SCHEMA_VERSION: u32 = PREVIOUS_PROJECTION_STATE_SCHEMA_VERSION;
}

/// One secret-free capability-to-canonical alias transaction entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionAlias {
    /// Opaque capability identifier.
    pub capability_id: String,
    /// Opaque canonical account identifier.
    pub canonical_account_id: String,
}

/// Atomic durable store for one canonical projection publication.
#[derive(Debug, Clone)]
pub struct FileProjectionStateStore {
    path: PathBuf,
}

impl FileProjectionStateStore {
    /// Construct the projection envelope path under a host data directory.
    #[must_use]
    pub fn under_data_dir(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("usage-broker").join("projection.json"),
        }
    }

    /// Read one exact v3 envelope. Corrupt bytes are quarantined; valid older
    /// and future schema versions return a non-destructive migration error.
    /// The broker uses a separate migration-only loader for v2 before any
    /// projection is exposed.
    pub fn load(&self) -> Result<Option<ProjectionStateEnvelope>, StateStoreError> {
        self.load_with_legacy_schema(false)
    }

    /// Read one exact current envelope or the immediately previous schema for
    /// the broker's one-time projection migration. Callers must normalize and
    /// persist v2 before exposing its projection. Valid future versions are
    /// reported without being quarantined or overwritten.
    pub(crate) fn load_for_broker_migration(
        &self,
    ) -> Result<Option<ProjectionStateEnvelope>, StateStoreError> {
        self.load_with_legacy_schema(true)
    }

    fn load_with_legacy_schema(
        &self,
        allow_previous_schema: bool,
    ) -> Result<Option<ProjectionStateEnvelope>, StateStoreError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(StateStoreError::Unavailable),
        };
        let value = match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(value @ serde_json::Value::Object(_)) => value,
            Ok(_) | Err(_) => {
                self.quarantine()?;
                return Err(StateStoreError::Corrupt);
            }
        };
        let Some(schema_version) = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
        else {
            self.quarantine()?;
            return Err(StateStoreError::Corrupt);
        };
        if schema_version > u64::from(PROJECTION_STATE_SCHEMA_VERSION) {
            return Err(StateStoreError::SchemaMigrationRequired {
                found: schema_version,
                current: PROJECTION_STATE_SCHEMA_VERSION,
            });
        }
        if schema_version == u64::from(PREVIOUS_PROJECTION_STATE_SCHEMA_VERSION)
            && !allow_previous_schema
        {
            self.decode_validated_envelope(value)?;
            return Err(StateStoreError::SchemaMigrationRequired {
                found: schema_version,
                current: PROJECTION_STATE_SCHEMA_VERSION,
            });
        }
        if schema_version != u64::from(PROJECTION_STATE_SCHEMA_VERSION)
            && !(allow_previous_schema
                && schema_version == u64::from(PREVIOUS_PROJECTION_STATE_SCHEMA_VERSION))
        {
            self.quarantine()?;
            return Err(StateStoreError::Corrupt);
        }
        let envelope = self.decode_validated_envelope(value)?;
        Ok(Some(envelope))
    }

    fn decode_validated_envelope(
        &self,
        value: serde_json::Value,
    ) -> Result<ProjectionStateEnvelope, StateStoreError> {
        let envelope = match serde_json::from_value::<ProjectionStateEnvelope>(value) {
            Ok(envelope) if envelope.projection.validate().is_ok() => envelope,
            Ok(_) | Err(_) => {
                self.quarantine()?;
                return Err(StateStoreError::Corrupt);
            }
        };
        Ok(envelope)
    }

    /// Atomically replace one publication envelope and sync its directory.
    pub fn store(&self, envelope: &ProjectionStateEnvelope) -> Result<(), StateStoreError> {
        if envelope.schema_version != ProjectionStateEnvelope::SCHEMA_VERSION {
            return Err(StateStoreError::Corrupt);
        }
        let envelope = envelope.clone();
        envelope
            .projection
            .validate()
            .map_err(|_| StateStoreError::Corrupt)?;
        let bytes = serde_json::to_vec(&envelope).map_err(|_| StateStoreError::Corrupt)?;
        let Some(parent) = self.path.parent() else {
            return Err(StateStoreError::Unavailable);
        };
        fs::create_dir_all(parent).map_err(|_| StateStoreError::Unavailable)?;
        let temporary = format!(
            ".projection.{}.{}.tmp",
            std::process::id(),
            STATE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let directory = open(
            parent,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
            Mode::empty(),
        )
        .map_err(|_| StateStoreError::Unavailable)?;
        let directory = File::from(directory);
        let fd = openat(
            &directory,
            temporary.as_str(),
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW,
            Mode::from_bits_truncate(0o600),
        )
        .map_err(|_| StateStoreError::Unavailable)?;
        let mut file = File::from(fd);
        file.write_all(&bytes)
            .map_err(|_| StateStoreError::Unavailable)?;
        file.sync_all().map_err(|_| StateStoreError::Unavailable)?;
        let filename = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(StateStoreError::Unavailable)?;
        renameat(&directory, temporary.as_str(), &directory, filename)
            .map_err(|_| StateStoreError::Unavailable)?;
        fsync(&directory).map_err(|_| StateStoreError::Unavailable)
    }

    pub(crate) fn quarantine(&self) -> Result<(), StateStoreError> {
        let suffix = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let quarantined = self.path.with_extension(format!("corrupt.{suffix}"));
        fs::rename(&self.path, quarantined).map_err(|_| StateStoreError::Unavailable)
    }
}

/// Directory-relative, no-follow host account store.
#[derive(Debug, Clone)]
pub struct FileAccountStateStore {
    accounts_dir: PathBuf,
}

impl FileAccountStateStore {
    /// Construct the broker account-state path under a host data directory.
    #[must_use]
    pub fn under_data_dir(data_dir: &Path) -> Self {
        Self {
            accounts_dir: data_dir.join("usage-broker").join("accounts"),
        }
    }

    /// Construct a store at an explicit test/operator-owned directory.
    #[must_use]
    pub fn at(accounts_dir: impl Into<PathBuf>) -> Self {
        Self {
            accounts_dir: accounts_dir.into(),
        }
    }

    pub(crate) fn open_accounts_dir(&self) -> Result<File, StateStoreError> {
        fs::create_dir_all(&self.accounts_dir).map_err(|_| StateStoreError::Unavailable)?;
        let fd = open(
            &self.accounts_dir,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
            Mode::empty(),
        )
        .map_err(|_| StateStoreError::Unavailable)?;
        let directory = File::from(fd);
        directory
            .set_permissions(fs::Permissions::from_mode(0o700))
            .map_err(|_| StateStoreError::Unavailable)?;
        validate_owned_mode(&directory, 0o700)?;
        Ok(directory)
    }
}

impl AccountStateStore for FileAccountStateStore {
    fn load(
        &self,
        capability: &UsageAccountCapability,
        now_epoch: i64,
    ) -> Result<Option<AccountStateEnvelope>, StateStoreError> {
        validate_capability(capability)?;
        let directory = self.open_accounts_dir()?;
        let filename = state_filename(capability);
        let fd = match openat(
            &directory,
            filename.as_str(),
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(nix::errno::Errno::ENOENT) => return Ok(None),
            Err(_) => return Err(StateStoreError::Unavailable),
        };
        let mut file = File::from(fd);
        validate_owned_mode(&file, 0o600)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_ACCOUNT_STATE_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|_| StateStoreError::Unavailable)?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_ACCOUNT_STATE_BYTES {
            return Err(StateStoreError::Corrupt);
        }
        let mut value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| StateStoreError::Corrupt)?;
        let schema_version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
            .ok_or(StateStoreError::Corrupt)?;
        let (envelope, migrated) = match schema_version {
            PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION => {
                // v1 recorded queue admission as the attempt start. The actual
                // provider start cannot be reconstructed, so retain the account
                // and conservatively begin a fresh floor from migration time.
                value
                    .as_object_mut()
                    .ok_or(StateStoreError::Corrupt)?
                    .insert("provider_invoked_at_epoch".into(), serde_json::Value::Null);
                let mut envelope: AccountStateEnvelope =
                    serde_json::from_value(value).map_err(|_| StateStoreError::Corrupt)?;
                envelope.provider_invoked_at_epoch = envelope
                    .started_at_epoch
                    .map(|admitted| admitted.max(now_epoch));
                envelope.schema_version = ACCOUNT_STATE_SCHEMA_VERSION;
                (envelope, true)
            }
            ACCOUNT_STATE_SCHEMA_VERSION => (
                serde_json::from_value(value).map_err(|_| StateStoreError::Corrupt)?,
                false,
            ),
            _ => return Err(StateStoreError::Corrupt),
        };
        let envelope = validate_envelope(envelope, capability, now_epoch)?;
        if migrated {
            self.store(&envelope, now_epoch)?;
        }
        Ok(Some(envelope))
    }

    fn store(
        &self,
        envelope: &AccountStateEnvelope,
        now_epoch: i64,
    ) -> Result<(), StateStoreError> {
        validate_capability(&envelope.capability)?;
        let mut envelope = sanitize_envelope(envelope.clone());
        envelope.schema_version = ACCOUNT_STATE_SCHEMA_VERSION;
        let expected = envelope.capability.clone();
        let envelope = validate_envelope(envelope, &expected, now_epoch)?;
        let bytes = serde_json::to_vec(&envelope).map_err(|_| StateStoreError::Corrupt)?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_ACCOUNT_STATE_BYTES {
            return Err(StateStoreError::Corrupt);
        }

        let directory = self.open_accounts_dir()?;
        let filename = state_filename(&envelope.capability);
        let temporary = format!(
            ".{filename}.{}.{}.tmp",
            std::process::id(),
            STATE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let fd = openat(
            &directory,
            temporary.as_str(),
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW,
            Mode::from_bits_truncate(0o600),
        )
        .map_err(|_| StateStoreError::Unavailable)?;
        let mut file = File::from(fd);
        let write_result = (|| {
            file.write_all(&bytes)
                .map_err(|_| StateStoreError::Unavailable)?;
            file.sync_all().map_err(|_| StateStoreError::Unavailable)?;
            renameat(
                &directory,
                temporary.as_str(),
                &directory,
                filename.as_str(),
            )
            .map_err(|_| StateStoreError::Unavailable)?;
            fsync(&directory).map_err(|_| StateStoreError::Unavailable)
        })();
        if write_result.is_err() {
            let _ignored_cleanup_result =
                unlinkat(&directory, temporary.as_str(), UnlinkatFlags::NoRemoveDir);
        }
        write_result
    }

    fn purge(&self, capability: &UsageAccountCapability) -> Result<(), StateStoreError> {
        validate_capability(capability)?;
        let directory = self.open_accounts_dir()?;
        let filename = state_filename(capability);
        match unlinkat(&directory, filename.as_str(), UnlinkatFlags::NoRemoveDir) {
            Ok(()) => fsync(&directory).map_err(|_| StateStoreError::Unavailable),
            Err(nix::errno::Errno::ENOENT) => Ok(()),
            Err(_) => Err(StateStoreError::Unavailable),
        }
    }

    fn quarantine(&self, capability: &UsageAccountCapability) -> Result<(), StateStoreError> {
        validate_capability(capability)?;
        let directory = self.open_accounts_dir()?;
        let filename = state_filename(capability);
        let suffix = format!(
            "{}-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
            STATE_QUARANTINE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let quarantined = format!(".{filename}.corrupt.{suffix}");
        match renameat(
            &directory,
            filename.as_str(),
            &directory,
            quarantined.as_str(),
        ) {
            Ok(()) => fsync(&directory).map_err(|_| StateStoreError::Unavailable),
            Err(nix::errno::Errno::ENOENT) => Ok(()),
            Err(_) => Err(StateStoreError::Unavailable),
        }
    }
}

impl FileProjectionStateStore {
    /// Remove the active envelope after a failed first publication. The
    /// caller uses this only to restore an absent preimage.
    pub fn clear(&self) -> Result<(), StateStoreError> {
        let Some(parent) = self.path.parent() else {
            return Err(StateStoreError::Unavailable);
        };
        let directory = match open(
            parent,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
            Mode::empty(),
        ) {
            Ok(directory) => directory,
            Err(nix::errno::Errno::ENOENT) => return Ok(()),
            Err(_) => return Err(StateStoreError::Unavailable),
        };
        let directory = File::from(directory);
        let filename = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(StateStoreError::Unavailable)?;
        match unlinkat(&directory, filename, UnlinkatFlags::NoRemoveDir) {
            Ok(()) => fsync(&directory).map_err(|_| StateStoreError::Unavailable),
            Err(nix::errno::Errno::ENOENT) => Ok(()),
            Err(_) => Err(StateStoreError::Unavailable),
        }
    }
}
