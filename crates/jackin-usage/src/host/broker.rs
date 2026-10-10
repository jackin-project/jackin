// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-only usage broker lifecycle and bounded Unix-socket transport.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _, PermissionsExt as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jackin_protocol::control::UsageSnapshotStatus;
use jackin_protocol::usage_broker::{
    USAGE_BROKER_MAX_FRAME_BYTES, USAGE_BROKER_PROTOCOL_VERSION, UsageAccountCapability,
    UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind, UsageCredentialScope,
    UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageProjectionV1,
};
use nix::fcntl::{OFlag, open, openat};
use nix::sys::stat::{Mode, fchmod, mkdirat};
use nix::unistd::{UnlinkatFlags, fsync, geteuid, unlinkat};
use sha2::{Digest as _, Sha256};

use jackin_protocol::usage_broker::UsageIdentityKindV1;

use crate::coordinator::{
    FileProjectionStateStore, ProjectionStateEnvelope, ProviderProbeOutcome,
    UsageCoordinatorConfig, UsageProviderExecutor,
};

use super::accounts::CanonicalAccountSubject;
use super::discovery::{
    ProviderCredentialEnvResolver, ProviderCredentialRefreshOutcome, ValidatedCredentialBinding,
    discover_usage_sources, refresh_credential_binding, validate_usage_sources,
};
use super::{HostSurfaceId, UsageDiscoveryScope, ValidatedUsageDiscovery};

const BROKER_DIR: &str = "usage-broker";
const BROKER_RUN_DIR: &str = "run";
const BROKER_SOCKET: &str = "usage-broker.sock";
const BROKER_LEADER: &str = "leader.pid";
/// Bounded CAS-conflict retries for broker-owned catalog refresh.
const BROKER_CATALOG_ATTEMPTS: u32 = 3;
const BROKER_LEASE_DURATION: Duration = Duration::from_secs(30);
const BROKER_LEASE_RENEWAL: Duration = Duration::from_secs(10);
const BROKER_IDLE_EXIT: Duration = Duration::from_mins(10);
const CONNECT_RETRY: Duration = Duration::from_secs(10);
const CONNECT_RETRY_STEP: Duration = Duration::from_millis(20);
const BROKER_CONNECTION_WORKERS: usize = 4;
const BROKER_CONNECTION_QUEUE: usize = 128;
const PUBLISH_TICK: Duration = Duration::from_millis(200);
/// Maximum `sun_path` bytes including the trailing NUL. `bind`/`connect`
/// fail beyond this, so over-long broker socket paths fall back to a
/// deterministic short alias (macOS allows 104, Linux 108).
#[cfg(target_os = "macos")]
pub(crate) const UNIX_SOCKET_PATH_LIMIT: usize = 104;
/// Maximum `sun_path` bytes including the trailing NUL. `bind`/`connect`
/// fail beyond this, so over-long broker socket paths fall back to a
/// deterministic short alias (macOS allows 104, Linux 108).
#[cfg(not(target_os = "macos"))]
pub(crate) const UNIX_SOCKET_PATH_LIMIT: usize = 108;
/// Alias directory prefix under the system temp dir, suffixed with the
/// effective uid so alias sockets stay per-user.
const BROKER_SOCKET_ALIAS_DIR_PREFIX: &str = "jk-ub-";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct BrokerLease {
    instance_id: String,
    process_id: u32,
    protocol_version: String,
    build_id: String,
    renewed_at_epoch: i64,
}

#[derive(Debug, Clone, Copy)]
struct ServePolicy {
    idle_exit: Duration,
    lease_duration: Duration,
    lease_renewal: Duration,
}

impl BrokerLease {
    #[cfg(test)]
    fn new(build_id: &str) -> Self {
        Self::new_at(build_id, chrono::Utc::now().timestamp())
    }

    fn new_at(build_id: &str, now_epoch: i64) -> Self {
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let instance_id = jackin_core::account_key_hash(
            "usage-broker-instance-v1",
            &format!("{}:{now_nanos}", std::process::id()),
        );
        Self {
            instance_id,
            process_id: std::process::id(),
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: build_id.to_owned(),
            renewed_at_epoch: now_epoch,
        }
    }
}

/// Descriptor-bound broker authority. Its exclusive file lock remains held
/// for the complete owner lifetime, so elapsed wall time cannot authorize a
/// contender to replace a live owner's lease.
///
/// Lifecycle operations verify that the path still names this descriptor's
/// inode and that the payload still names this instance before updating or
/// removing it. A stale process holding an old descriptor therefore cannot
/// renew or unlink a replacement lease at the same path.
struct BrokerLeaseOwner {
    lease: BrokerLease,
    file: File,
    stale_lease_reclaimed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BrokerSocketIdentity {
    device: u64,
    inode: u64,
}

/// Owns the startup lease and bound socket identity until the serve loop takes
/// over. Drop removes only the still-matching socket inode while the
/// descriptor-bound lease is locked, then removes that exact lease inode.
struct BrokerStartupCleanup {
    lease_path: PathBuf,
    socket_path: PathBuf,
    lease: Option<BrokerLeaseOwner>,
    socket_identity: Option<BrokerSocketIdentity>,
}

impl BrokerStartupCleanup {
    fn new(lease_path: PathBuf, socket_path: PathBuf, lease: BrokerLeaseOwner) -> Self {
        Self {
            lease_path,
            socket_path,
            lease: Some(lease),
            socket_identity: None,
        }
    }

    fn reclaimed_stale_lease(&self) -> bool {
        self.lease
            .as_ref()
            .is_some_and(|lease| lease.stale_lease_reclaimed)
    }

    fn record_socket_identity(&mut self) -> Result<(), ()> {
        let metadata = fs::symlink_metadata(&self.socket_path).map_err(|_| ())?;
        if !metadata.file_type().is_socket() {
            return Err(());
        }
        self.socket_identity = Some(BrokerSocketIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        });
        Ok(())
    }

    fn renew(&mut self, _lease_duration: Duration) -> bool {
        self.lease
            .as_mut()
            .is_some_and(|lease| renew_lease(&self.lease_path, lease))
    }
}

impl Drop for BrokerStartupCleanup {
    fn drop(&mut self) {
        if let Some(lease) = self.lease.as_mut() {
            let _ignored = cleanup_owned_files(
                &self.lease_path,
                &self.socket_path,
                self.socket_identity,
                lease,
            );
        }
    }
}

/// Host broker filesystem and handshake configuration.
#[derive(Debug, Clone)]
pub struct UsageBrokerConfig {
    /// Host-only jackin data directory. Never mounted into a Capsule.
    pub data_dir: PathBuf,
    /// Exact caller build identifier.
    pub build_id: String,
    /// Bounded refresh scheduling policy.
    pub coordinator: UsageCoordinatorConfig,
    /// Idle lifetime after the last client while no generation is active.
    pub idle_exit: Duration,
    /// Lease lifetime used by the process-independent authority.
    pub lease_duration: Duration,
    /// Lease renewal cadence.
    pub lease_renewal: Duration,
    /// Optional sibling broker executable used by process activation.
    pub service_executable: Option<PathBuf>,
}

impl UsageBrokerConfig {
    /// Build production defaults for one host data directory.
    #[must_use]
    pub fn for_data_dir(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            build_id: env!("CARGO_PKG_VERSION").to_owned(),
            coordinator: UsageCoordinatorConfig::default(),
            idle_exit: BROKER_IDLE_EXIT,
            lease_duration: BROKER_LEASE_DURATION,
            lease_renewal: BROKER_LEASE_RENEWAL,
            service_executable: default_service_executable(),
        }
    }

    fn socket_path(&self) -> PathBuf {
        let full = self
            .data_dir
            .join(BROKER_DIR)
            .join(BROKER_RUN_DIR)
            .join(BROKER_SOCKET);
        short_socket_alias(&full).unwrap_or(full)
    }

    /// Construct a fail-closed client even when broker startup is unavailable.
    #[must_use]
    pub fn client(&self) -> UsageBrokerClient {
        UsageBrokerClient::at_host(
            self.socket_path(),
            self.data_dir.clone(),
            self.build_id.clone(),
        )
    }
}

/// Deterministic short alias for a broker socket path that exceeds the
/// platform `sun_path` limit (deep test tempdirs, long `$HOME`).
///
/// Returns `None` when the full path fits or the alias directory cannot be
/// provisioned; callers then use the full path and fail closed exactly as
/// before. Client and server derive the same alias from the same
/// `data_dir`, so no rendezvous state is needed. The alias directory is
/// per-uid, `0700`, and ownership-validated like the run directory; the
/// socket file itself keeps the existing `0600` + ownership checks at bind
/// time. Distinct data directories map to distinct alias names via the
/// 64-bit SHA-256 prefix of the full path.
pub(crate) fn short_socket_alias(full: &Path) -> Option<PathBuf> {
    if full.as_os_str().len() < UNIX_SOCKET_PATH_LIMIT {
        return None;
    }
    let digest = Sha256::digest(full.as_os_str().as_encoded_bytes());
    let mut name = String::with_capacity(21);
    name.push_str("jk-");
    for byte in digest.iter().take(8) {
        use std::fmt::Write as _;
        let _written = write!(name, "{byte:02x}");
    }
    name.push_str(".sock");
    let dir = std::env::temp_dir().join(format!(
        "{BROKER_SOCKET_ALIAS_DIR_PREFIX}{}",
        geteuid().as_raw()
    ));
    fs::create_dir_all(&dir).ok()?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).ok()?;
    let metadata = fs::symlink_metadata(&dir).ok()?;
    if metadata.file_type().is_symlink()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return None;
    }
    let alias = dir.join(name);
    if alias.as_os_str().len() >= UNIX_SOCKET_PATH_LIMIT {
        return None;
    }
    Some(alias)
}

fn default_service_executable() -> Option<PathBuf> {
    std::env::var_os("JACKIN_USAGE_BROKER_BIN")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok().and_then(|path| {
                path.parent()
                    .map(|parent| parent.join("jackin-usage-broker"))
            })
        })
}

/// Secret-free launch facts proving which credential sources reached a Capsule.
pub(super) fn usage_catalog_entries(discovery: &ValidatedUsageDiscovery) -> Vec<UsageCatalogEntry> {
    let mut source_revisions = BTreeMap::<UsageAccountCapability, BTreeSet<String>>::new();
    for binding in &discovery.bindings {
        let capability = capability_for_binding(binding, discovery.config_generation.as_deref());
        source_revisions
            .entry(capability)
            .or_default()
            .insert(format!(
                "{}:{}:{}:{}",
                binding.capability_id.len(),
                binding.capability_id,
                binding.credential_revision.len(),
                binding.credential_revision,
            ));
    }
    source_revisions
        .into_iter()
        .map(|(capability, source_revisions)| {
            let revision_material = source_revisions
                .iter()
                .map(|source_revision| format!("{}:{source_revision}", source_revision.len()))
                .collect::<Vec<_>>()
                .join("|");
            UsageCatalogEntry {
                revision: jackin_core::account_key_hash(
                    "usage-catalog-entry-v3",
                    &revision_material,
                ),
                capability,
            }
        })
        .collect()
}

fn empty_projection(build_id: &str) -> UsageProjectionV1 {
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: format!("{build_id}:empty"),
        generated_at_epoch: chrono::Utc::now().timestamp(),
        discovery_revision: "empty".to_owned(),
        broker_instance_id: jackin_core::account_key_hash(
            "usage-broker-instance-v1",
            &format!("{}:{build_id}", std::process::id()),
        ),
        broker_generation: 0,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

struct LoadedProjection {
    projection: Arc<Mutex<UsageProjectionV1>>,
    catalog: Option<Vec<UsageCatalogEntry>>,
    catalog_revision: Option<String>,
}

fn load_projection(config: &UsageBrokerConfig) -> Result<LoadedProjection, UsageCoordinationError> {
    let store = FileProjectionStateStore::under_data_dir(&config.data_dir);
    let loaded = match store.load_for_broker_migration() {
        Ok(loaded) => loaded,
        // v1, invalid envelopes, and unsupported schemas are quarantined by
        // the store. The broker deliberately rebuilds an empty projection
        // from current discovery; unavailable state is not safe to overwrite.
        Err(crate::coordinator::StateStoreError::Corrupt) => None,
        Err(
            crate::coordinator::StateStoreError::Unavailable
            | crate::coordinator::StateStoreError::SchemaMigrationRequired { .. },
        ) => return Err(unavailable()),
    };
    let has_loaded_envelope = loaded.is_some();
    let (projection, envelope) = if let Some(mut envelope) = loaded {
        if envelope.schema_version == ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION {
            publish::migrate_legacy_projection(&mut envelope.projection)
                .map_err(|_| unavailable())?;
            envelope.schema_version = ProjectionStateEnvelope::SCHEMA_VERSION;
        }
        (envelope.projection.clone(), envelope)
    } else {
        let projection = empty_projection(&config.build_id);
        let envelope = ProjectionStateEnvelope {
            schema_version: ProjectionStateEnvelope::SCHEMA_VERSION,
            catalog_revision: projection.discovery_revision.clone(),
            catalog: Vec::new(),
            broker_instance_id: projection.broker_instance_id.clone(),
            projection: projection.clone(),
            aliases: Vec::new(),
            retry_deadline_epoch: None,
            success_deadline_epoch: None,
        };
        (projection, envelope)
    };
    store.store(&envelope).map_err(|_| unavailable())?;
    let catalog = has_loaded_envelope.then(|| envelope.catalog.clone());
    let catalog_revision = has_loaded_envelope.then(|| envelope.catalog_revision.clone());
    Ok(LoadedProjection {
        projection: Arc::new(Mutex::new(projection)),
        catalog,
        catalog_revision,
    })
}

#[cfg(test)]
type TestClaudeCollector =
    dyn Fn(&UsageAccountCapability, &str) -> ProviderProbeOutcome + Send + Sync;

struct DiscoveryProviderExecutor {
    bindings: Mutex<BTreeMap<UsageAccountCapability, Vec<ValidatedCredentialBinding>>>,
    validated_catalog: Mutex<Option<StagedDiscoveryCatalog>>,
    scope: UsageDiscoveryScope,
    resolver: Arc<dyn ProviderCredentialEnvResolver>,
    monitor_store: Option<Arc<monitor::MonitorStore>>,
    collector_service: Option<String>,
    collector_liveness: Option<Arc<crate::usage::ClaudeCollectorLiveness>>,
    #[cfg(test)]
    claude_collector: Option<Arc<TestClaudeCollector>>,
    probe_budget: Duration,
}

struct EmptyProviderCredentialResolver;

impl ProviderCredentialEnvResolver for EmptyProviderCredentialResolver {
    fn resolve_provider_credentials(
        &self,
        _config: &jackin_config::AppConfig,
        _workspace: Option<&jackin_core::WorkspaceName>,
        _role: Option<&str>,
        _keys: &[jackin_core::UsageCredentialEnvName],
    ) -> Vec<super::discovery::ProviderCredentialEnvResolution> {
        Vec::new()
    }
}

struct StagedDiscoveryCatalog {
    catalog_revision: String,
    entries: BTreeMap<UsageAccountCapability, String>,
    discovery: ValidatedUsageDiscovery,
}

fn probe_with_scope(
    executor: &DiscoveryProviderExecutor,
    capability: &UsageAccountCapability,
    launch_scope: Option<&UsageCredentialScope>,
) -> ProviderProbeOutcome {
    if executor.collector_service.is_some() && capability.surface_id != "claude" {
        return collector_not_authorized();
    }
    if capability.surface_id == "claude" {
        return probe_claude_with_scope(executor, capability);
    }
    // The coordinator only classifies elapsed time after a probe returns, so
    // the blocking provider call (child CLI/RPC, secret resolution) runs under
    // an explicit broker-side budget. Expiry completes the generation through
    // the normal failure path: last-good quota is preserved and broker
    // ownership is unaffected.
    let cached = executor
        .bindings
        .lock()
        .ok()
        .and_then(|bindings| bindings.get(capability).cloned());
    let scope = executor.scope.clone();
    let resolver = Arc::clone(&executor.resolver);
    let task_capability = capability.clone();
    let launch_scope = launch_scope.cloned();
    let outcome = probe::run_probe_with_budget(executor.probe_budget, move || {
        let (bindings, refreshed) = match cached {
            Some(bindings) => (Some(bindings), None),
            None => rediscover_bindings(&scope, resolver.as_ref(), &task_capability),
        };
        let binding = bindings.as_deref().and_then(|bindings| {
            launch_scope.as_ref().map_or_else(
                || unscoped_refresh_binding(bindings),
                |scope| {
                    authorize_credential_binding_group(bindings, &task_capability.surface_id, scope)
                },
            )
        });
        let outcome = match binding {
            Some(binding) => refresh_binding_outcome(&binding, resolver.as_ref()),
            None => ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::Unauthorized,
                message: "usage account capability is not authorized".to_owned(),
                retry_at_epoch: None,
            },
        };
        (outcome, refreshed)
    });
    match outcome {
        Ok((outcome, refreshed)) => {
            if let Some(refreshed) = refreshed
                && let Ok(mut bindings) = executor.bindings.lock()
            {
                *bindings = refreshed;
            }
            outcome
        }
        Err(_) => probe::probe_timeout_outcome(),
    }
}

fn probe_claude_with_scope(
    executor: &DiscoveryProviderExecutor,
    capability: &UsageAccountCapability,
) -> ProviderProbeOutcome {
    let Some(service) = executor.collector_service.as_deref() else {
        return collector_auth_required();
    };
    let Some(monitor_store) = executor.monitor_store.as_ref() else {
        return collector_not_authorized();
    };
    if !monitor_store
        .collection_accounts()
        .iter()
        .any(|account_id| account_id == &capability.account_id)
        || claude_usage_capability_for_service(service) != *capability
    {
        return collector_not_authorized();
    }

    let service = service.to_owned();
    let consent_monitor_store = Arc::clone(monitor_store);
    let consent_service = service.clone();
    let consent_capability = capability.clone();
    let authorized_source: Arc<dyn Fn(u64) -> bool + Send + Sync> = Arc::new(move |generation| {
        crate::usage::claude_credential_generation_is_current(&consent_service, generation)
            && claude_usage_capability_for_service(&consent_service) == consent_capability
            && consent_monitor_store
                .collection_accounts()
                .iter()
                .any(|account_id| account_id == &consent_capability.account_id)
    });
    let consent_liveness = executor.collector_liveness.as_ref().map(Arc::clone);
    let liveness_for_current = consent_liveness.as_ref().map(Arc::clone);
    let source_for_current = Arc::clone(&authorized_source);
    let consent_is_current = move || {
        liveness_for_current.as_ref().is_some_and(|liveness| {
            liveness.is_current_if(|generation| source_for_current(generation))
        })
    };
    let liveness_for_admission = consent_liveness;
    let admit_operation = move || {
        liveness_for_admission.as_ref().and_then(|liveness| {
            let authorized_source = Arc::clone(&authorized_source);
            liveness.admit_if(move |generation| authorized_source(generation))
        })
    };

    #[cfg(test)]
    if let Some(collector) = executor.claude_collector.as_ref() {
        let lifecycle_is_current = executor
            .collector_liveness
            .as_ref()
            .is_some_and(|liveness| liveness.is_current());
        return if lifecycle_is_current
            && claude_usage_capability_for_service(&service) == *capability
            && monitor_store
                .collection_accounts()
                .iter()
                .any(|account_id| account_id == &capability.account_id)
        {
            collector(capability, &service)
        } else {
            collector_not_authorized()
        };
    }

    match probe::run_probe_with_budget(executor.probe_budget, move || {
        let result = crate::usage::experimental_claude_usage_snapshot_for_service(
            "claude",
            Some("Claude"),
            chrono::Utc::now().timestamp(),
            &service,
            admit_operation,
            consent_is_current,
        );
        match result {
            Ok(Some(snapshot)) => provider_probe_outcome_with_metadata(
                snapshot.view,
                snapshot.rate_limit,
                snapshot.failure_metadata,
            ),
            Ok(None) => collector_auth_required(),
            Err(crate::usage::ClaudeCollectionError::ConsentRevoked {
                provider_http_status: None,
            }) => collector_not_authorized(),
            Err(crate::usage::ClaudeCollectionError::ConsentRevoked {
                provider_http_status: Some(http_status),
            }) => {
                let mut view = jackin_protocol::control::FocusedUsageView::unavailable(
                    "claude",
                    chrono::Utc::now().timestamp(),
                );
                view.status = if http_status == 401 {
                    UsageSnapshotStatus::NeedsSecret
                } else {
                    UsageSnapshotStatus::Error
                };
                view.last_error = Some(format!(
                    "Claude authorization changed after HTTP {http_status}; prepare the selected source again"
                ));
                provider_probe_outcome_with_metadata(
                    view,
                    None,
                    Some(crate::usage::ProviderFailureMetadata {
                        kind: crate::usage::ProviderErrorKind::HttpStatus,
                        http_status: Some(http_status),
                    }),
                )
            }
        }
    }) {
        Ok(outcome) => outcome,
        Err(_) => probe::probe_timeout_outcome(),
    }
}

fn collector_auth_required() -> ProviderProbeOutcome {
    ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::NeedsSecret,
        message: "Claude collection requires foreground authentication preparation".to_owned(),
        retry_at_epoch: None,
    }
}

fn collector_not_authorized() -> ProviderProbeOutcome {
    ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::Unauthorized,
        message: "Claude collection is not authorized for this source".to_owned(),
        retry_at_epoch: None,
    }
}

fn claude_usage_capability_for_service(service: &str) -> UsageAccountCapability {
    let source_capability_id = crate::usage::claude_source_capability_id_for_service(service);
    let identity = super::accounts::CanonicalAccountIdentity::source_capability(
        HostSurfaceId::Claude,
        &source_capability_id,
    );
    let subject = identity.account_key();
    let hashed = jackin_core::account_key_hash("claude", &subject);
    UsageAccountCapability {
        surface_id: "claude".to_owned(),
        account_id: hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned(),
    }
}

fn foreground_catalog_revision(service: &str, entries: &[UsageCatalogEntry]) -> String {
    let source_capability_id = crate::usage::claude_source_capability_id_for_service(service);
    let mut material = String::new();
    push_catalog_revision_component(&mut material, &source_capability_id);
    material.push_str(&entries.len().to_string());
    material.push(':');
    let mut ordered_entries = entries.iter().collect::<Vec<_>>();
    ordered_entries.sort_by(|left, right| left.capability.cmp(&right.capability));
    for entry in ordered_entries {
        push_catalog_revision_component(&mut material, &entry.capability.surface_id);
        push_catalog_revision_component(&mut material, &entry.capability.account_id);
        push_catalog_revision_component(&mut material, &entry.revision);
    }
    jackin_core::account_key_hash("usage-foreground-claude-catalog-v2", &material)
}

fn push_catalog_revision_component(material: &mut String, value: &str) {
    material.push_str(&value.len().to_string());
    material.push(':');
    material.push_str(value);
}

fn validate_foreground_catalog_revision(
    service: &str,
    catalog_revision: &str,
    entries: &[UsageCatalogEntry],
) -> Result<(), UsageCoordinationError> {
    // Foreground auth intentionally installs one source-scoped Claude row.
    // Its revision is derived from this exact row and service; it must never
    // trigger full host discovery or admit unrelated persisted capabilities.
    let expected = claude_usage_capability_for_service(service);
    if entries.len() != 1
        || entries[0].capability != expected
        || entries[0].revision.is_empty()
        || foreground_catalog_revision(service, entries) != catalog_revision
    {
        return Err(catalog_discovery_mismatch());
    }
    Ok(())
}

impl UsageProviderExecutor for DiscoveryProviderExecutor {
    fn authorize_credential_scope(
        &self,
        capability: &UsageAccountCapability,
        scope: &UsageCredentialScope,
    ) -> Result<(), UsageCoordinationError> {
        let bindings = self
            .bindings
            .lock()
            .map_err(|_| unavailable())?
            .get(capability)
            .cloned()
            .ok_or_else(credential_scope_mismatch)?;
        authorize_credential_binding_group(&bindings, &capability.surface_id, scope)
            .map(|_| ())
            .ok_or_else(credential_scope_mismatch)
    }

    fn probe(&self, capability: &UsageAccountCapability, _generation: u64) -> ProviderProbeOutcome {
        probe_with_scope(self, capability, None)
    }

    fn probe_scoped(
        &self,
        capability: &UsageAccountCapability,
        _generation: u64,
        scope: &UsageCredentialScope,
    ) -> ProviderProbeOutcome {
        probe_with_scope(self, capability, Some(scope))
    }

    fn reconcile_catalog(
        &self,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        if self.collector_service.is_some() {
            self.bindings.lock().map_err(|_| unavailable())?.clear();
            return Ok(());
        }
        let admitted = entries
            .iter()
            .map(|entry| entry.capability.clone())
            .collect::<BTreeSet<_>>();
        // The caller's catalog is authoritative. A transient discovery failure
        // must clear old bindings rather than leave a revoked credential
        // usable; a later probe can rediscover one admitted capability.
        let mut bindings =
            rediscover_all_bindings(&self.scope, self.resolver.as_ref()).unwrap_or_default();
        bindings.retain(|capability, _| admitted.contains(capability));
        self.bindings
            .lock()
            .map_err(|_| unavailable())?
            .clone_from(&bindings);
        Ok(())
    }

    fn validate_catalog(
        &self,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        if self.collector_service.is_some() {
            return Err(catalog_discovery_mismatch());
        }
        let Some(discovery) = rediscover_discovery(&self.scope, self.resolver.as_ref()) else {
            return Err(unavailable());
        };
        let expected = usage_catalog_entries(&discovery)
            .into_iter()
            .map(|entry| (entry.capability, entry.revision))
            .collect::<BTreeMap<_, _>>();
        let observed = entries
            .iter()
            .map(|entry| (entry.capability.clone(), entry.revision.clone()))
            .collect::<BTreeMap<_, _>>();
        if expected == observed {
            Ok(())
        } else {
            Err(catalog_discovery_mismatch())
        }
    }

    fn validate_catalog_revision(
        &self,
        catalog_revision: &str,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        if let Some(service) = self.collector_service.as_deref() {
            return validate_foreground_catalog_revision(service, catalog_revision, entries);
        }
        let Some(discovery) = rediscover_discovery(&self.scope, self.resolver.as_ref()) else {
            return Err(unavailable());
        };
        ensure_catalog_matches(&discovery, catalog_revision, entries)?;
        self.validated_catalog
            .lock()
            .map_err(|_| unavailable())?
            .replace(StagedDiscoveryCatalog {
                catalog_revision: catalog_revision.to_owned(),
                entries: catalog_entry_map(entries),
                discovery,
            });
        Ok(())
    }

    fn reconcile_catalog_revision(
        &self,
        catalog_revision: &str,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        if self.collector_service.is_some() {
            // Claude probes use the retained exact-service credential lease,
            // not a discovered credential binding. Any removed catalog rows
            // are fenced by the coordinator's persisted cooldown tombstones.
            self.bindings.lock().map_err(|_| unavailable())?.clear();
            return Ok(());
        }
        let requested_entries = catalog_entry_map(entries);
        let staged = self
            .validated_catalog
            .lock()
            .map_err(|_| unavailable())?
            .take()
            .filter(|staged| {
                staged.catalog_revision == catalog_revision && staged.entries == requested_entries
            });
        let discovery = if let Some(staged) = staged {
            staged.discovery
        } else {
            let Some(discovery) = rediscover_discovery(&self.scope, self.resolver.as_ref()) else {
                return Err(unavailable());
            };
            ensure_catalog_matches(&discovery, catalog_revision, entries)?;
            discovery
        };
        let admitted = entries
            .iter()
            .map(|entry| entry.capability.clone())
            .collect::<BTreeSet<_>>();
        // Preserve every binding in a canonical capability group. Profile
        // sources remain first for refresh selection, while authorization
        // checks every relevant env proof against the complete group.
        let mut bindings = grouped_bindings(&discovery);
        bindings.retain(|capability, _| admitted.contains(capability));
        self.bindings
            .lock()
            .map_err(|_| unavailable())?
            .clone_from(&bindings);
        Ok(())
    }
}

fn rediscover_discovery(
    scope: &UsageDiscoveryScope,
    resolver: &dyn ProviderCredentialEnvResolver,
) -> Option<ValidatedUsageDiscovery> {
    discover_usage_sources(scope, resolver)
        .ok()
        .map(|catalog| validate_usage_sources(catalog, resolver))
}

fn rediscover_all_bindings(
    scope: &UsageDiscoveryScope,
    resolver: &dyn ProviderCredentialEnvResolver,
) -> Option<BTreeMap<UsageAccountCapability, Vec<ValidatedCredentialBinding>>> {
    rediscover_discovery(scope, resolver).map(|discovery| grouped_bindings(&discovery))
}

type RediscoveredBindings = (
    Option<Vec<ValidatedCredentialBinding>>,
    Option<BTreeMap<UsageAccountCapability, Vec<ValidatedCredentialBinding>>>,
);

fn rediscover_bindings(
    scope: &UsageDiscoveryScope,
    resolver: &dyn ProviderCredentialEnvResolver,
    capability: &UsageAccountCapability,
) -> RediscoveredBindings {
    let bindings = rediscover_all_bindings(scope, resolver);
    let Some(bindings) = bindings else {
        return (None, None);
    };
    let Some(group) = bindings.get(capability).cloned() else {
        // A successful scan that cannot reproduce the requested capability is
        // a catalog mismatch. Do not replace the cache with a partial scan;
        // the caller must fail closed for this exact capability.
        return (None, None);
    };
    (Some(group), Some(bindings))
}

fn grouped_bindings(
    discovery: &ValidatedUsageDiscovery,
) -> BTreeMap<UsageAccountCapability, Vec<ValidatedCredentialBinding>> {
    let mut bindings = BTreeMap::new();
    for binding in &discovery.bindings {
        bindings
            .entry(capability_for_binding(
                binding,
                discovery.config_generation.as_deref(),
            ))
            .or_insert_with(Vec::new)
            .push(binding.clone());
    }
    bindings
}

fn catalog_entry_map(entries: &[UsageCatalogEntry]) -> BTreeMap<UsageAccountCapability, String> {
    entries
        .iter()
        .map(|entry| (entry.capability.clone(), entry.revision.clone()))
        .collect()
}

fn ensure_catalog_matches(
    discovery: &ValidatedUsageDiscovery,
    catalog_revision: &str,
    entries: &[UsageCatalogEntry],
) -> Result<(), UsageCoordinationError> {
    let service_revision = discovery.config_generation.as_deref().unwrap_or("empty");
    let service_entries = usage_catalog_entries(discovery);
    if service_revision != catalog_revision
        || catalog_entry_map(&service_entries) != catalog_entry_map(entries)
    {
        return Err(catalog_discovery_mismatch());
    }
    Ok(())
}

fn refresh_binding_outcome(
    binding: &ValidatedCredentialBinding,
    resolver: &dyn ProviderCredentialEnvResolver,
) -> ProviderProbeOutcome {
    match refresh_credential_binding(binding, resolver) {
        ProviderCredentialRefreshOutcome::Snapshot {
            view,
            rate_limit,
            failure_metadata,
        } => provider_probe_outcome_with_metadata(*view, rate_limit, failure_metadata),
        ProviderCredentialRefreshOutcome::Missing
        | ProviderCredentialRefreshOutcome::Denied
        | ProviderCredentialRefreshOutcome::InteractionRequired => ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::NeedsSecret,
            message: "usage provider credentials require operator action".to_owned(),
            retry_at_epoch: None,
        },
        ProviderCredentialRefreshOutcome::Malformed => ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::ProviderUnavailable,
            message: "usage provider response is unavailable".to_owned(),
            retry_at_epoch: None,
        },
    }
}

#[cfg(test)]
fn provider_probe_outcome(
    view: jackin_protocol::control::FocusedUsageView,
) -> ProviderProbeOutcome {
    provider_probe_outcome_with_rate_limit(view, None)
}

#[cfg(test)]
fn provider_probe_outcome_with_rate_limit(
    view: jackin_protocol::control::FocusedUsageView,
    rate_limit: Option<crate::usage::ProviderRateLimit>,
) -> ProviderProbeOutcome {
    provider_probe_outcome_with_metadata(view, rate_limit, None)
}

fn provider_probe_outcome_with_metadata(
    view: jackin_protocol::control::FocusedUsageView,
    rate_limit: Option<crate::usage::ProviderRateLimit>,
    failure_metadata: Option<crate::usage::ProviderFailureMetadata>,
) -> ProviderProbeOutcome {
    if let Some(rate_limit) = rate_limit {
        return ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::RateLimited,
            message: "usage provider rate limit is active".to_owned(),
            retry_at_epoch: rate_limit.retry_at_epoch,
        };
    }
    if let Some(failure_metadata) = failure_metadata {
        use crate::usage::ProviderErrorKind;

        let kind = match failure_metadata.kind {
            ProviderErrorKind::Timeout => UsageCoordinationErrorKind::ProviderTimeout,
            ProviderErrorKind::HttpStatus => match failure_metadata.http_status {
                Some(401) => UsageCoordinationErrorKind::NeedsSecret,
                Some(403) => UsageCoordinationErrorKind::Unauthorized,
                Some(429) => UsageCoordinationErrorKind::RateLimited,
                _ => UsageCoordinationErrorKind::ProviderUnavailable,
            },
            ProviderErrorKind::Transport | ProviderErrorKind::Decode | ProviderErrorKind::Other => {
                UsageCoordinationErrorKind::ProviderUnavailable
            }
        };
        let fallback = match kind {
            UsageCoordinationErrorKind::NeedsSecret => {
                "usage provider credentials require operator action"
            }
            UsageCoordinationErrorKind::Unauthorized => {
                "usage provider denied the configured credential"
            }
            UsageCoordinationErrorKind::RateLimited => "usage provider rate limit is active",
            UsageCoordinationErrorKind::ProviderTimeout => "usage provider request timed out",
            _ => "usage provider quota is unavailable",
        };
        return ProviderProbeOutcome::Failure {
            kind,
            message: honest_probe_message(&view, fallback),
            retry_at_epoch: None,
        };
    }
    match view.status {
        UsageSnapshotStatus::NeedsSecret | UsageSnapshotStatus::NeedsLogin => {
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::NeedsSecret,
                message: honest_probe_message(
                    &view,
                    "usage provider credentials require operator action",
                ),
                retry_at_epoch: None,
            }
        }
        UsageSnapshotStatus::Error
        | UsageSnapshotStatus::Unavailable
        | UsageSnapshotStatus::Stale => ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::ProviderUnavailable,
            message: honest_probe_message(&view, "usage provider quota is unavailable"),
            retry_at_epoch: None,
        },
        UsageSnapshotStatus::Unsupported => ProviderProbeOutcome::success(view),
        UsageSnapshotStatus::Fresh => ProviderProbeOutcome::success(view),
    }
}

/// Carry the collector's specific gap reason into a probe failure.
///
/// Failure kinds stay stable for retry matching; only the operator-facing
/// message becomes specific. Views without their own reason keep the generic
/// fallback.
fn honest_probe_message(
    view: &jackin_protocol::control::FocusedUsageView,
    fallback: &str,
) -> String {
    view.last_error
        .as_deref()
        .filter(|message| !message.trim().is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

mod catalog;
mod catalog_diagnostics;
mod client;
mod client_security;
mod credential_scope;
mod dispatch_ops;
mod monitor;
mod monitor_api;
mod probe;
pub(super) mod publish;
mod serve_loop;
mod service;
mod view;
mod waits;

pub use client::UsageBrokerClient;
pub use credential_scope::ForwardedUsageSources;
use credential_scope::{authorize_credential_binding_group, unscoped_refresh_binding};
pub(super) use credential_scope::{
    forwarded_usage_capabilities, usage_capability_for_selected_account_with_sources,
};
#[cfg(test)]
pub(super) use credential_scope::{
    usage_broker_capabilities, usage_capability_for_selected_account,
};
pub use monitor::parse_statusline;
pub use service::{
    UsageBrokerForegroundReady, ensure_usage_broker_process, ensure_usage_broker_with_executor,
    run_usage_broker_foreground_bootstrap, run_usage_broker_service,
    run_usage_broker_service_with_executor,
};

#[cfg(test)]
fn write_with_deadline(stream: &mut UnixStream, bytes: &[u8], timeout: Duration) {
    serve_loop::write_with_deadline(stream, bytes, timeout);
}

fn read_frame<T: serde::de::DeserializeOwned>(
    stream: &mut UnixStream,
) -> Result<T, UsageCoordinationError> {
    let mut reader = BufReader::new(stream);
    let mut bytes = Vec::new();
    let read = reader
        .by_ref()
        .take(u64::try_from(USAGE_BROKER_MAX_FRAME_BYTES).unwrap_or(u64::MAX) + 1)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| unavailable())?;
    if read == 0 || read > USAGE_BROKER_MAX_FRAME_BYTES || bytes.last() != Some(&b'\n') {
        return Err(protocol_error());
    }
    bytes.pop();
    serde_json::from_slice(&bytes).map_err(|_| protocol_error())
}

fn secure_run_directory(data_dir: &Path) -> Result<PathBuf, UsageCoordinationError> {
    fs::create_dir_all(data_dir).map_err(|_| unavailable())?;
    let data_fd = open(
        data_dir,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| unavailable())?;
    let data = File::from(data_fd);
    validate_owned_base_directory(&data)?;
    let broker = private_child_directory(&data, BROKER_DIR)?;
    let run = private_child_directory(&broker, BROKER_RUN_DIR)?;
    drop(run);
    Ok(data_dir.join(BROKER_DIR).join(BROKER_RUN_DIR))
}

fn private_child_directory(parent: &File, name: &str) -> Result<File, UsageCoordinationError> {
    match mkdirat(parent, name, Mode::from_bits_truncate(0o700)) {
        Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
        Err(_) => return Err(unavailable()),
    }
    let fd = openat(
        parent,
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| unavailable())?;
    let directory = File::from(fd);
    fchmod(&directory, Mode::from_bits_truncate(0o700)).map_err(|_| unavailable())?;
    validate_owned_directory(&directory)?;
    Ok(directory)
}

fn validate_owned_directory(directory: &File) -> Result<(), UsageCoordinationError> {
    let metadata = directory.metadata().map_err(|_| unavailable())?;
    if !metadata.is_dir()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err(unavailable());
    }
    Ok(())
}

fn validate_owned_base_directory(directory: &File) -> Result<(), UsageCoordinationError> {
    let metadata = directory.metadata().map_err(|_| unavailable())?;
    if !metadata.is_dir() || metadata.uid() != geteuid().as_raw() || metadata.mode() & 0o022 != 0 {
        return Err(unavailable());
    }
    Ok(())
}

fn validate_owned_mode(path: &Path, mode: u32) -> Result<(), UsageCoordinationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| unavailable())?;
    if metadata.file_type().is_symlink()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != mode
    {
        return Err(unavailable());
    }
    Ok(())
}

fn claim_leader(
    path: &Path,
    build_id: &str,
    lease_duration: Duration,
) -> Result<Option<BrokerLeaseOwner>, UsageCoordinationError> {
    claim_leader_at(
        path,
        build_id,
        lease_duration,
        chrono::Utc::now().timestamp(),
    )
}

fn claim_leader_at(
    path: &Path,
    build_id: &str,
    lease_duration: Duration,
    now_epoch: i64,
) -> Result<Option<BrokerLeaseOwner>, UsageCoordinationError> {
    let lease = BrokerLease::new_at(build_id, now_epoch);
    loop {
        match open(
            path,
            OFlag::O_RDWR | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => {
                let mut file = File::from(fd);
                validate_owned_file(&file, 0o600)?;
                // The owner retains this lock until its descriptor is dropped.
                // A contender cannot infer that an owner is dead from a stale
                // timestamp while the kernel still reports the lock held.
                if file.try_lock().is_err() {
                    return Ok(None);
                }
                if !lease_path_matches_open_file(path, &file) {
                    drop(file);
                    continue;
                }
                let result =
                    claim_existing_lease(&mut file, &lease, build_id, lease_duration, now_epoch);
                return match result {
                    // Keep the descriptor lock in the returned owner.
                    Ok(Some(stale_lease_reclaimed)) => {
                        if !lease_path_matches_open_file(path, &file) {
                            return Err(unavailable());
                        }
                        Ok(Some(BrokerLeaseOwner {
                            lease,
                            file,
                            stale_lease_reclaimed,
                        }))
                    }
                    Ok(None) => {
                        file.unlock().map_err(|_| unavailable())?;
                        Ok(None)
                    }
                    Err(error) => Err(error),
                };
            }
            Err(nix::errno::Errno::ENOENT) => {
                return create_new_lease(path, lease).map(Some);
            }
            Err(_) => return Err(unavailable()),
        }
    }
}

fn create_new_lease(
    path: &Path,
    lease: BrokerLease,
) -> Result<BrokerLeaseOwner, UsageCoordinationError> {
    let fd = open(
        path,
        OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )
    .map_err(|_| unavailable())?;
    let mut file = File::from(fd);
    let result = (|| {
        validate_owned_file(&file, 0o600)?;
        file.try_lock().map_err(|_| unavailable())?;
        if !lease_path_matches_open_file(path, &file) {
            return Err(unavailable());
        }
        write_lease(&mut file, &lease).map_err(|_| unavailable())?;
        if !lease_path_matches_open_file(path, &file) {
            return Err(unavailable());
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ignored = unlink_created_lease(path, &mut file);
        return Err(error);
    }
    Ok(BrokerLeaseOwner {
        lease,
        file,
        stale_lease_reclaimed: false,
    })
}

fn claim_existing_lease(
    file: &mut File,
    replacement: &BrokerLease,
    build_id: &str,
    lease_duration: Duration,
    now_epoch: i64,
) -> Result<Option<bool>, UsageCoordinationError> {
    if file.metadata().map_err(|_| unavailable())?.nlink() == 0 {
        return Ok(None);
    }
    let bytes = read_lease_bytes(file).map_err(|_| unavailable())?;
    let existing = serde_json::from_slice::<BrokerLease>(&bytes).ok();
    let replace = if let Some(existing) = existing {
        if existing.protocol_version != USAGE_BROKER_PROTOCOL_VERSION
            || existing.build_id != build_id
        {
            // A healthy incompatible endpoint is never replaced by an
            // activator; the client will receive protocol_mismatch.
            return Ok(None);
        }
        now_epoch.saturating_sub(existing.renewed_at_epoch)
            >= i64::try_from(lease_duration.as_secs()).unwrap_or(i64::MAX)
    } else {
        // Unknown or legacy formats do not carry enough ownership data to
        // authorize replacing the lease path.
        return Ok(None);
    };
    if !replace {
        return Ok(None);
    }
    write_lease(file, replacement).map_err(|_| unavailable())?;
    Ok(Some(true))
}

fn renew_lease(path: &Path, owner: &mut BrokerLeaseOwner) -> bool {
    renew_lease_at(path, owner, chrono::Utc::now().timestamp())
}

fn renew_lease_at(path: &Path, owner: &mut BrokerLeaseOwner, now_epoch: i64) -> bool {
    if !lease_path_matches_open_file(path, &owner.file) {
        return false;
    }
    (|| {
        let mut current = read_lease(&mut owner.file).ok()?;
        if current.instance_id != owner.lease.instance_id
            || current.process_id != owner.lease.process_id
            || owner.lease.process_id != std::process::id()
            || current.protocol_version != owner.lease.protocol_version
            || current.build_id != owner.lease.build_id
            || current.renewed_at_epoch != owner.lease.renewed_at_epoch
            || !lease_path_matches_open_file(path, &owner.file)
        {
            return Some(false);
        }
        // The lifetime lock proves that this process still owns the lease.
        // A long sleep may make the timestamp old, but must not fence a live
        // owner from renewing after it wakes.
        current.renewed_at_epoch = now_epoch.max(current.renewed_at_epoch);
        write_lease(&mut owner.file, &current).ok()?;
        if !lease_path_matches_open_file(path, &owner.file) {
            return Some(false);
        }
        owner.lease.renewed_at_epoch = current.renewed_at_epoch;
        Some(true)
    })()
    .unwrap_or(false)
}

fn lease_path_matches_open_file(path: &Path, file: &File) -> bool {
    let Ok(descriptor_metadata) = file.metadata() else {
        return false;
    };
    let Ok(path_metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    descriptor_metadata.nlink() != 0
        && descriptor_metadata.is_file()
        && path_metadata.is_file()
        && !path_metadata.file_type().is_symlink()
        && path_metadata.dev() == descriptor_metadata.dev()
        && path_metadata.ino() == descriptor_metadata.ino()
}

fn cleanup_owned_files(
    lease_path: &Path,
    socket_path: &Path,
    socket_identity: Option<BrokerSocketIdentity>,
    owner: &mut BrokerLeaseOwner,
) -> bool {
    (|| -> Result<(), ()> {
        if !lease_path_matches_open_file(lease_path, &owner.file) {
            return Err(());
        }
        let current = read_lease(&mut owner.file).map_err(|_| ())?;
        if current.instance_id != owner.lease.instance_id
            || current.protocol_version != owner.lease.protocol_version
            || current.build_id != owner.lease.build_id
        {
            return Err(());
        }
        // The lease descriptor remains locked while the owned startup socket
        // and lease are removed. Never remove a socket path that this startup
        // did not bind, or whose inode has since been replaced.
        if let Some(identity) = socket_identity {
            if !lease_path_matches_open_file(lease_path, &owner.file) {
                return Err(());
            }
            unlink_owned_socket_path(socket_path, identity)?;
        }
        if !lease_path_matches_open_file(lease_path, &owner.file) {
            return Err(());
        }
        unlink_owned_path(lease_path)?;
        Ok(())
    })()
    .is_ok()
}

fn unlink_owned_socket_path(path: &Path, expected: BrokerSocketIdentity) -> Result<(), ()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Ok(()),
    };
    if !metadata.file_type().is_socket()
        || metadata.dev() != expected.device
        || metadata.ino() != expected.inode
    {
        return Ok(());
    }
    unlink_owned_path(path)
}

fn unlink_owned_path(path: &Path) -> Result<(), ()> {
    let parent = path.parent().ok_or(())?;
    let filename = path.file_name().and_then(|name| name.to_str()).ok_or(())?;
    let directory = open(
        parent,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| ())?;
    let directory = File::from(directory);
    match unlinkat(&directory, filename, UnlinkatFlags::NoRemoveDir) {
        Ok(()) | Err(nix::errno::Errno::ENOENT) => {}
        Err(_) => return Err(()),
    }
    fsync(&directory).map_err(|_| ())?;
    Ok(())
}

fn unlink_created_lease(path: &Path, file: &mut File) -> bool {
    let Ok(expected) = file.metadata() else {
        return false;
    };
    let Ok(actual) = fs::symlink_metadata(path) else {
        return false;
    };
    if actual.file_type().is_symlink()
        || actual.dev() != expected.dev()
        || actual.ino() != expected.ino()
    {
        return false;
    }
    unlink_owned_path(path).is_ok()
}

fn read_lease(file: &mut File) -> Result<BrokerLease, std::io::Error> {
    let bytes = read_lease_bytes(file)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn read_lease_bytes(file: &mut File) -> Result<Vec<u8>, std::io::Error> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn write_lease(file: &mut File, lease: &BrokerLease) -> Result<(), std::io::Error> {
    let bytes = serde_json::to_vec(lease)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.sync_all()
}

fn validate_owned_file(file: &File, mode: u32) -> Result<(), UsageCoordinationError> {
    let metadata = file.metadata().map_err(|_| unavailable())?;
    if metadata.uid() != geteuid().as_raw() || metadata.mode() & 0o777 != mode {
        return Err(unavailable());
    }
    Ok(())
}

fn wait_for_leader(client: &UsageBrokerClient) -> Result<(), UsageCoordinationError> {
    let started = Instant::now();
    while started.elapsed() < CONNECT_RETRY {
        if connect_probe(client) {
            return Ok(());
        }
        std::thread::park_timeout(CONNECT_RETRY_STEP);
    }
    Err(unavailable())
}

fn connect_probe(client: &UsageBrokerClient) -> bool {
    client.probe_current_projection()
}

pub(super) fn capability_for_binding(
    binding: &ValidatedCredentialBinding,
    catalog_revision: Option<&str>,
) -> UsageAccountCapability {
    let subject = if let Some(identity) = &binding.identity {
        identity.account_key()
    } else {
        format!("provisional-capability-v1:{}", binding.capability_id)
    };
    let stable_claude_source = binding.surface == HostSurfaceId::Claude
        && matches!(
            binding.identity.as_ref().map(|identity| &identity.subject),
            Some(CanonicalAccountSubject::SourceCapability(_))
        );
    let subject = match (catalog_revision, stable_claude_source) {
        (Some(_), true) => subject,
        (Some(revision), false) => format!(
            "usage-capability-v2:catalog-revision:{}:{revision}:subject:{}:{subject}",
            revision.len(),
            subject.len()
        ),
        (None, _) => subject,
    };
    let hashed = jackin_core::account_key_hash(binding.surface.id(), &subject);
    let account_id = hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned();
    UsageAccountCapability {
        account_id,
        surface_id: binding.surface.id().to_owned(),
    }
}

/// Preserve the canonical identity evidence that host discovery already
/// merged before the broker publisher turns generation views into the
/// Capsule-facing projection. Labels are deliberately not consulted.
fn publication_identity_metadata(
    discovery: &ValidatedUsageDiscovery,
) -> BTreeMap<UsageAccountCapability, publish::AccountIdentityMetadata> {
    let mut evidence =
        BTreeMap::<UsageAccountCapability, (UsageIdentityKindV1, BTreeSet<String>)>::new();
    for binding in &discovery.bindings {
        let capability = capability_for_binding(binding, discovery.config_generation.as_deref());
        let identity_kind = match binding.identity.as_ref().map(|identity| &identity.subject) {
            Some(CanonicalAccountSubject::ProviderId(_)) => UsageIdentityKindV1::ProviderAccountId,
            Some(CanonicalAccountSubject::ProviderStableHandle(_)) => {
                UsageIdentityKindV1::ProviderStableHandle
            }
            Some(CanonicalAccountSubject::SourceCapability(_)) => {
                UsageIdentityKindV1::LocalSourceHandle
            }
            None => UsageIdentityKindV1::UnverifiedHandle,
        };
        let entry = evidence
            .entry(capability)
            .or_insert_with(|| (identity_kind, BTreeSet::new()));
        // Keep the strongest exact identity evidence when several sources
        // coalesce onto one capability. Missing evidence never upgrades to a
        // provider or local-source claim.
        if identity_kind_strength(identity_kind) > identity_kind_strength(entry.0) {
            entry.0 = identity_kind;
        }
        entry.1.extend(binding.provenance.iter().cloned());
    }
    evidence
        .into_iter()
        .map(|(capability, (identity_kind, provenance))| {
            (
                capability,
                publish::AccountIdentityMetadata {
                    identity_kind,
                    provenance_count: u32::try_from(provenance.len()).unwrap_or(u32::MAX),
                },
            )
        })
        .collect()
}

fn identity_kind_strength(identity_kind: UsageIdentityKindV1) -> u8 {
    match identity_kind {
        UsageIdentityKindV1::UnverifiedHandle => 0,
        UsageIdentityKindV1::LocalSourceHandle => 1,
        UsageIdentityKindV1::ProviderStableHandle => 2,
        UsageIdentityKindV1::ProviderAccountId => 3,
    }
}

fn projection_identity_metadata(
    projection: &UsageProjectionV1,
) -> BTreeMap<UsageAccountCapability, publish::AccountIdentityMetadata> {
    projection
        .providers
        .iter()
        .flat_map(|provider| {
            provider.accounts.iter().map(|account| {
                (
                    UsageAccountCapability {
                        surface_id: publish::surface_id_for_provider(&provider.provider_id),
                        account_id: account.canonical_account_id.clone(),
                    },
                    publish::AccountIdentityMetadata {
                        identity_kind: account.identity_kind,
                        provenance_count: account.provenance_count,
                    },
                )
            })
        })
        .collect()
}

fn unavailable() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::Unavailable,
        message: "usage broker is unavailable".to_owned(),
    }
}

fn broker_conflict() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::BrokerConflict,
        message: "usage broker lease is already owned or cannot be safely identified".to_owned(),
    }
}

fn credential_scope_mismatch() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::Unauthorized,
        message: "launch credential source no longer matches staged material".to_owned(),
    }
}

fn protocol_error() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::ProtocolMismatch,
        message: "usage broker protocol mismatch".to_owned(),
    }
}

fn catalog_discovery_mismatch() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::CatalogRevisionConflict,
        message: "usage broker catalog does not match current discovery".to_owned(),
    }
}

#[cfg(test)]
mod tests;
