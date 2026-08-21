// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Atomic host-only refresh-generation persistence.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use jackin_protocol::control::FocusedUsageView;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageProjectionV1, UsageRefreshPhase,
};
use nix::fcntl::{OFlag, open, openat, renameat};
use nix::sys::stat::Mode;
use nix::unistd::{UnlinkatFlags, fsync, geteuid, unlinkat};
use serde::{Deserialize, Serialize};

const ACCOUNT_STATE_SCHEMA_VERSION: u32 = 1;
const MAX_ACCOUNT_STATE_BYTES: u64 = 512 * 1024;
const MAX_CLOCK_SKEW_SECS: i64 = 300;
const MAX_DISPLAY_CHARS: usize = 256;
const PROJECTION_STATE_SCHEMA_VERSION: u32 = 1;
static STATE_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Complete durable state for one canonical account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountStateEnvelope {
    /// Persisted schema version.
    pub schema_version: u32,
    /// Canonical account authority.
    pub capability: UsageAccountCapability,
    /// Monotonic account generation.
    pub generation: u64,
    /// Refresh lifecycle phase.
    pub phase: UsageRefreshPhase,
    /// Terminal result for this generation, when data-bearing.
    pub terminal_result: Option<FocusedUsageView>,
    /// Last independently data-bearing provider result.
    pub last_good: Option<FocusedUsageView>,
    /// Sanitized terminal failure.
    pub terminal_error: Option<UsageCoordinationError>,
    /// Generation start timestamp.
    pub started_at_epoch: Option<i64>,
    /// Generation completion timestamp.
    pub completed_at_epoch: Option<i64>,
    /// Provider-mandated rate-limit deadline.
    pub rate_limit_deadline_epoch: Option<i64>,
    /// Provider-supplied general retry deadline.
    pub retry_deadline_epoch: Option<i64>,
    /// Ambient success-cooldown deadline.
    pub success_deadline_epoch: Option<i64>,
    /// Consecutive provider failure count.
    pub consecutive_failures: u32,
}

impl AccountStateEnvelope {
    /// Initial idle state for a newly discovered account.
    #[must_use]
    pub fn idle(capability: UsageAccountCapability) -> Self {
        Self {
            schema_version: ACCOUNT_STATE_SCHEMA_VERSION,
            capability,
            generation: 0,
            phase: UsageRefreshPhase::Idle,
            terminal_result: None,
            last_good: None,
            terminal_error: None,
            started_at_epoch: None,
            completed_at_epoch: None,
            rate_limit_deadline_epoch: None,
            retry_deadline_epoch: None,
            success_deadline_epoch: None,
            consecutive_failures: 0,
        }
    }
}

/// Sanitized persistence failure.
#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub enum StateStoreError {
    /// Host state path, owner, or permissions are unavailable.
    #[error("usage coordinator state is unavailable")]
    Unavailable,
    /// Envelope bytes or schema failed validation.
    #[error("usage coordinator state is corrupt")]
    Corrupt,
}

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
    /// Broker-owned retry deadline.
    pub retry_deadline_epoch: Option<i64>,
    /// Broker-owned success/cadence deadline.
    pub success_deadline_epoch: Option<i64>,
    /// Process incarnation that published this envelope.
    pub broker_instance_id: String,
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

    /// Read one validated envelope. Corrupt bytes are quarantined and treated
    /// as unavailable rather than being rendered or used for provider work.
    pub fn load(&self) -> Result<Option<ProjectionStateEnvelope>, StateStoreError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(StateStoreError::Unavailable),
        };
        let envelope = match serde_json::from_slice::<ProjectionStateEnvelope>(&bytes) {
            Ok(envelope) if envelope.schema_version == PROJECTION_STATE_SCHEMA_VERSION => envelope,
            Ok(_) | Err(_) => {
                self.quarantine();
                return Err(StateStoreError::Corrupt);
            }
        };
        envelope
            .projection
            .validate()
            .map_err(|_| StateStoreError::Corrupt)?;
        Ok(Some(envelope))
    }

    /// Atomically replace one publication envelope and sync its directory.
    pub fn store(&self, envelope: &ProjectionStateEnvelope) -> Result<(), StateStoreError> {
        let mut envelope = envelope.clone();
        envelope.schema_version = PROJECTION_STATE_SCHEMA_VERSION;
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

    fn quarantine(&self) {
        let suffix = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let quarantined = self.path.with_extension(format!("corrupt.{suffix}"));
        let _ignored = fs::rename(&self.path, quarantined);
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

    fn open_accounts_dir(&self) -> Result<File, StateStoreError> {
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
        let envelope: AccountStateEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| StateStoreError::Corrupt)?;
        validate_envelope(envelope, capability, now_epoch).map(Some)
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
}

fn validate_owned_mode(file: &File, expected: u32) -> Result<(), StateStoreError> {
    let metadata = file.metadata().map_err(|_| StateStoreError::Unavailable)?;
    if metadata.uid() != geteuid().as_raw() || metadata.mode() & 0o777 != expected {
        return Err(StateStoreError::Unavailable);
    }
    Ok(())
}

fn validate_capability(capability: &UsageAccountCapability) -> Result<(), StateStoreError> {
    let valid = |value: &str| {
        !value.is_empty()
            && value.len() <= 128
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
    };
    if !valid(&capability.account_id) || !valid(&capability.surface_id) {
        return Err(StateStoreError::Corrupt);
    }
    Ok(())
}

fn state_filename(capability: &UsageAccountCapability) -> String {
    format!("{}-{}.json", capability.surface_id, capability.account_id)
}

fn validate_envelope(
    envelope: AccountStateEnvelope,
    expected: &UsageAccountCapability,
    now_epoch: i64,
) -> Result<AccountStateEnvelope, StateStoreError> {
    if envelope.schema_version != ACCOUNT_STATE_SCHEMA_VERSION || &envelope.capability != expected {
        return Err(StateStoreError::Corrupt);
    }
    let future_limit = now_epoch.saturating_add(MAX_CLOCK_SKEW_SECS);
    if [
        envelope.started_at_epoch,
        envelope.completed_at_epoch,
        envelope
            .terminal_result
            .as_ref()
            .map(|view| view.fetched_at_epoch),
        envelope
            .last_good
            .as_ref()
            .map(|view| view.fetched_at_epoch),
    ]
    .into_iter()
    .flatten()
    .any(|timestamp| timestamp > future_limit)
    {
        return Err(StateStoreError::Corrupt);
    }
    Ok(sanitize_envelope(envelope))
}

fn sanitize_envelope(mut envelope: AccountStateEnvelope) -> AccountStateEnvelope {
    envelope.terminal_result = envelope.terminal_result.map(sanitize_usage_view);
    envelope.last_good = envelope.last_good.map(sanitize_usage_view);
    if let Some(error) = &mut envelope.terminal_error {
        error.message = sanitize_text(&error.message);
    }
    envelope
}

pub(super) fn sanitize_usage_view(mut view: FocusedUsageView) -> FocusedUsageView {
    view.focused_agent = view.focused_agent.map(|value| sanitize_text(&value));
    view.focused_provider = view.focused_provider.map(|value| sanitize_text(&value));
    view.account.provider_label = sanitize_text(&view.account.provider_label);
    view.account.account_label = sanitize_text(&view.account.account_label);
    view.account.username = view.account.username.map(|value| sanitize_text(&value));
    view.account.plan_label = view.account.plan_label.map(|value| sanitize_text(&value));
    view.account.credential_origin = view
        .account
        .credential_origin
        .map(|value| sanitize_text(&value));
    view.updated_label = sanitize_text(&view.updated_label);
    view.status_bar_label = sanitize_text(&view.status_bar_label);
    view.last_error = view.last_error.map(|value| sanitize_text(&value));
    for bucket in &mut view.buckets {
        bucket.label = sanitize_text(&bucket.label);
        bucket.used_label = bucket.used_label.take().map(|value| sanitize_text(&value));
        bucket.limit_label = bucket.limit_label.take().map(|value| sanitize_text(&value));
        bucket.reset_label = bucket.reset_label.take().map(|value| sanitize_text(&value));
        bucket.pace_label = bucket.pace_label.take().map(|value| sanitize_text(&value));
        if let Some(money) = &mut bucket.used_money {
            money.currency = sanitize_text(&money.currency);
        }
        if let Some(money) = &mut bucket.limit_money {
            money.currency = sanitize_text(&money.currency);
        }
    }
    for tab in &mut view.tabs {
        tab.label = sanitize_text(&tab.label);
        tab.status_label = sanitize_text(&tab.status_label);
        tab.account_label = sanitize_text(&tab.account_label);
        tab.plan_label = tab.plan_label.take().map(|value| sanitize_text(&value));
        tab.source_label = tab.source_label.take().map(|value| sanitize_text(&value));
    }
    view
}

fn sanitize_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_DISPLAY_CHARS)
        .collect()
}

#[cfg(test)]
mod tests;
