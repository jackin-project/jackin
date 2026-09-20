// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-only usage broker lifecycle and bounded Unix-socket transport.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jackin_protocol::control::UsageSnapshotStatus;
use jackin_protocol::usage_broker::{
    USAGE_BROKER_MAX_FRAME_BYTES, USAGE_BROKER_PROTOCOL_VERSION, UsageAccountCapability,
    UsageBrokerOperation, UsageBrokerRequest, UsageBrokerResponse, UsageCatalogEntry,
    UsageCoordinationError, UsageCoordinationErrorKind, UsageCredentialScope, UsageGenerationView,
    UsageIdentityKindV1, UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageProjectionV1,
    UsageRefreshPhase,
};
use nix::fcntl::{OFlag, open, openat, renameat};
use nix::sys::signal::kill;
use nix::sys::stat::{Mode, fchmod, mkdirat};
use nix::unistd::{Pid, UnlinkatFlags, fsync, geteuid, unlinkat};

use crate::coordinator::{
    FileAccountStateStore, FileProjectionStateStore, ProjectionStateEnvelope, ProviderProbeOutcome,
    UsageCoordinator, UsageCoordinatorConfig, UsageProviderExecutor,
};

use super::accounts::CanonicalAccountSubject;
use super::discovery::{
    ProviderCredentialEnvResolver, ProviderCredentialRefreshOutcome,
    ProviderCredentialSourceMaterial, ValidatedCredentialBinding, ValidatedCredentialSource,
    discover_usage_sources, refresh_credential_binding, validate_usage_sources,
};
use super::{HostSurfaceId, HostUsageRuntime, UsageDiscoveryScope, ValidatedUsageDiscovery};

impl HostUsageRuntime {
    /// Whether this runtime permits host broker provider work.
    #[must_use]
    pub fn live_probes_enabled(&self) -> bool {
        self.probe_policy == super::HostProbePolicy::Live
    }

    /// Whether any host broker generation remains active.
    #[must_use]
    pub fn broker_refresh_in_progress(&self) -> bool {
        self.broker_phases.values().any(|phase| phase.is_active())
    }

    /// Whether any active broker generation belongs to `surface_id`.
    #[must_use]
    pub fn surface_refresh_in_progress(&self, surface_id: &str) -> bool {
        self.broker_phases
            .iter()
            .any(|(capability, phase)| capability.surface_id == surface_id && phase.is_active())
    }

    /// Adopt one host-broker projection and never execute provider work here.
    pub fn apply_broker_generation(&mut self, state: UsageGenerationView) -> Result<(), String> {
        self.require_open()?;
        let capability = state.capability.clone();
        self.broker_phases.insert(capability.clone(), state.phase);
        if let Some(mut view) = state.snapshot {
            if let Some(error) = &state.error {
                view.last_error = Some(error.message.clone());
                view.status = if view.buckets.is_empty() {
                    UsageSnapshotStatus::Error
                } else {
                    UsageSnapshotStatus::Stale
                };
            }
            let binding = self.discovery.as_ref().and_then(|discovery| {
                discovery
                    .bindings
                    .iter()
                    .find(|binding| {
                        capability_for_binding(binding, discovery.config_generation.as_deref())
                            == capability
                    })
                    .cloned()
            });
            if let Some(binding) = binding {
                self.record_discovered_snapshot(&binding, view);
            }
        } else if let Some(error) = &state.error {
            self.push_event(
                "probe_failed",
                Some(&capability.surface_id),
                Some(error.message.clone()),
            );
        }
        if state.phase.is_terminal() {
            self.broker_phases.remove(&capability);
            self.last_refresh = Some(Instant::now());
        }
        self.push_event(
            "broker_phase_changed",
            Some(&capability.surface_id),
            Some(
                match state.phase {
                    UsageRefreshPhase::Idle => "idle",
                    UsageRefreshPhase::Queued => "queued",
                    UsageRefreshPhase::Updating => "updating",
                    UsageRefreshPhase::Completed => "completed",
                    UsageRefreshPhase::Failed => "failed",
                }
                .to_owned(),
            ),
        );
        Ok(())
    }

    /// Surface one coordination failure without discarding last-good quota.
    pub fn record_broker_error(
        &mut self,
        capability: &UsageAccountCapability,
        error: &UsageCoordinationError,
    ) -> Result<(), String> {
        self.require_open()?;
        if error.kind == UsageCoordinationErrorKind::CatalogRevoked
            && self.broker_phases.remove(capability).is_some()
        {
            self.push_event(
                "broker_phase_changed",
                Some(&capability.surface_id),
                Some("failed".to_owned()),
            );
        }
        self.push_event(
            "probe_failed",
            Some(&capability.surface_id),
            Some(error.message.clone()),
        );
        Ok(())
    }
}

const BROKER_DIR: &str = "usage-broker";
const BROKER_RUN_DIR: &str = "run";
const BROKER_SOCKET: &str = "usage-broker.sock";
const BROKER_LEADER: &str = "leader.pid";
const BROKER_LEASE_DURATION: Duration = Duration::from_secs(30);
const BROKER_LEASE_RENEWAL: Duration = Duration::from_secs(10);
const BROKER_IDLE_EXIT: Duration = Duration::from_mins(10);
const CONNECT_RETRY: Duration = Duration::from_secs(10);
const CONNECT_RETRY_STEP: Duration = Duration::from_millis(20);
const BROKER_CONNECTION_WORKERS: usize = 4;
const BROKER_CONNECTION_QUEUE: usize = 128;
const PUBLISH_TICK: Duration = Duration::from_millis(200);

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
    fn new(build_id: &str) -> Self {
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
            renewed_at_epoch: chrono::Utc::now().timestamp(),
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
        self.data_dir
            .join(BROKER_DIR)
            .join(BROKER_RUN_DIR)
            .join(BROKER_SOCKET)
    }

    /// Construct a fail-closed client even when broker startup is unavailable.
    #[must_use]
    pub fn client(&self) -> UsageBrokerClient {
        UsageBrokerClient::at(self.socket_path(), self.build_id.clone())
    }
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

/// Attached broker plus every host-discovered canonical capability.
#[derive(Debug, Clone)]
pub struct UsageBrokerHandle {
    /// Host-only transport client.
    pub client: UsageBrokerClient,
    /// Canonical accounts known to this discovery generation.
    pub capabilities: Vec<UsageAccountCapability>,
    /// Publication lease fencing this activation's capability set.
    pub catalog_lease: String,
    scoped_capabilities: BTreeMap<String, Vec<ScopedCapability>>,
}

impl UsageBrokerHandle {
    /// Exact canonical accounts whose credential source was forwarded at launch.
    #[must_use]
    pub fn capabilities_for_forwarded_scope(
        &self,
        scope_label: &str,
        sources: &ForwardedUsageSources,
    ) -> Vec<UsageAccountCapability> {
        self.scoped_capabilities
            .get(scope_label)
            .into_iter()
            .flatten()
            .filter(|entry| entry.requirement.is_forwarded(sources))
            .map(|entry| entry.capability.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

/// Secret-free launch facts proving which credential sources reached a Capsule.
#[derive(Debug, Clone, Default)]
pub struct ForwardedUsageSources {
    /// Exact configured account ids admitted to this Capsule. A provider
    /// surface alone is never sufficient when several accounts share it.
    pub selected_account_ids: BTreeSet<String>,
    /// Provider surface paired with each selected configured account id. This
    /// lets the runtime replace the config alias with the canonical authority
    /// discovered for that exact account.
    pub selected_account_surfaces: BTreeMap<String, String>,
    /// Surface ids with a successfully forwarded profile directory.
    pub profile_surface_ids: BTreeSet<String>,
    /// Governed provider env names present in the Capsule's resolved environment.
    pub env_keys: BTreeSet<String>,
    /// Exact source/material proofs staged for this launch.
    pub credential_scope: UsageCredentialScope,
}

#[derive(Debug, Clone)]
struct ScopedCapability {
    capability: UsageAccountCapability,
    requirement: ForwardingRequirement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ForwardingRequirement {
    Profile(String),
    Env {
        surface: String,
        key: String,
        account_ids: BTreeSet<String>,
        material: Option<ProviderCredentialSourceMaterial>,
    },
    Capability,
}

/// Match the staged consumer key to the broker's canonical discovery key.
///
/// The key is an agent/provider contract alias, not the protected source
/// identity. The source declaration, account, surface, and material
/// fingerprint remain exact; only the closed provider alias sets below may
/// bridge a launch-native key to the canonical discovery label.
fn credential_keys_match(surface: &str, canonical: &str, staged: &str) -> bool {
    if canonical == staged {
        return true;
    }
    let Some(surface) = HostSurfaceId::from_id(surface) else {
        return false;
    };
    match surface {
        HostSurfaceId::Kimi => matches!(
            (canonical, staged),
            (
                jackin_core::KIMI_CODE_API_KEY_ENV_NAME
                    | jackin_core::KIMI_API_KEY_ENV_NAME
                    | jackin_core::MOONSHOT_API_KEY_ENV_NAME
                    | jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME,
                jackin_core::KIMI_CODE_API_KEY_ENV_NAME
                    | jackin_core::KIMI_API_KEY_ENV_NAME
                    | jackin_core::MOONSHOT_API_KEY_ENV_NAME
                    | jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME
            )
        ),
        HostSurfaceId::Zai => matches!(
            (canonical, staged),
            (
                jackin_core::ZAI_API_KEY_ENV_NAME
                    | jackin_core::ZHIPU_API_KEY_ENV_NAME
                    | jackin_core::OPENAI_API_KEY_ENV_NAME
                    | jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME,
                jackin_core::ZAI_API_KEY_ENV_NAME
                    | jackin_core::ZHIPU_API_KEY_ENV_NAME
                    | jackin_core::OPENAI_API_KEY_ENV_NAME
                    | jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME
            )
        ),
        HostSurfaceId::Minimax => matches!(
            (canonical, staged),
            (
                jackin_core::MINIMAX_API_KEY_ENV_NAME | jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME,
                jackin_core::MINIMAX_API_KEY_ENV_NAME | jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME
            )
        ),
        HostSurfaceId::Google => matches!(
            (canonical, staged),
            (
                jackin_core::GEMINI_API_KEY_ENV_NAME | jackin_core::GOOGLE_API_KEY_ENV_NAME,
                jackin_core::GEMINI_API_KEY_ENV_NAME | jackin_core::GOOGLE_API_KEY_ENV_NAME
            )
        ),
        _ => false,
    }
}

fn credential_scope_matches(
    scope: &UsageCredentialScope,
    account_ids: &BTreeSet<String>,
    surface: &str,
    canonical_key: &str,
    material: &ProviderCredentialSourceMaterial,
) -> bool {
    let mut found = false;
    for proof in scope
        .sources
        .iter()
        .filter(|proof| account_ids.contains(&proof.account_id) && proof.surface_id == surface)
    {
        found = true;
        if !credential_keys_match(surface, canonical_key, &proof.key)
            || proof.source != material.source
            || proof.material_fingerprint != material.material_fingerprint
        {
            return false;
        }
    }
    found
}

impl ForwardingRequirement {
    fn is_forwarded(&self, sources: &ForwardedUsageSources) -> bool {
        match self {
            Self::Profile(surface) => sources.profile_surface_ids.contains(surface),
            Self::Env {
                surface,
                key,
                account_ids,
                material,
            } => {
                if sources.selected_account_ids.is_empty() {
                    return sources.env_keys.contains(key);
                }
                let Some(material) = material else {
                    return false;
                };
                let account_ids = account_ids
                    .iter()
                    .filter(|account_id| sources.selected_account_ids.contains(*account_id))
                    .cloned()
                    .collect::<BTreeSet<_>>();
                credential_scope_matches(
                    &sources.credential_scope,
                    &account_ids,
                    surface,
                    key,
                    material,
                )
            }
            Self::Capability => false,
        }
    }
}

fn forwarding_requirement(binding: &ValidatedCredentialBinding) -> ForwardingRequirement {
    match &binding.source {
        ValidatedCredentialSource::Profile(_) => {
            ForwardingRequirement::Profile(binding.surface.id().to_owned())
        }
        ValidatedCredentialSource::Env { key, material, .. } => ForwardingRequirement::Env {
            surface: binding.surface.id().to_owned(),
            key: key.clone(),
            account_ids: binding
                .provenance
                .iter()
                .filter_map(|provenance| provenance.strip_prefix("account "))
                .map(str::to_owned)
                .collect(),
            material: material.clone(),
        },
        ValidatedCredentialSource::Capability => ForwardingRequirement::Capability,
    }
}

/// Derive an exact per-container capability allowlist before broker startup.
#[must_use]
pub fn forwarded_usage_capabilities(
    discovery: &ValidatedUsageDiscovery,
    scope_label: &str,
    sources: &ForwardedUsageSources,
) -> Vec<UsageAccountCapability> {
    discovery
        .bindings
        .iter()
        .filter(|binding| {
            if sources.selected_account_ids.is_empty() {
                binding.provenance.contains(scope_label)
            } else {
                binding.provenance.iter().any(|provenance| {
                    sources
                        .selected_account_ids
                        .iter()
                        .any(|account_id| provenance == &format!("account {account_id}"))
                })
            }
        })
        .filter(|binding| forwarding_requirement(binding).is_forwarded(sources))
        .map(|binding| capability_for_binding(binding, discovery.config_generation.as_deref()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Resolve one exact configured account to the canonical broker capability
/// used by Capsule sessions. Multiple source bindings for the same canonical
/// account collapse to one capability; distinct identities are rejected rather
/// than guessed.
#[must_use]
pub fn usage_capability_for_selected_account(
    discovery: &ValidatedUsageDiscovery,
    account_id: &str,
    surface_id: &str,
) -> Option<UsageAccountCapability> {
    let provenance = format!("account {account_id}");
    let capabilities = discovery
        .bindings
        .iter()
        .filter(|binding| binding.surface.id() == surface_id)
        .filter(|binding| binding.provenance.contains(&provenance))
        .map(|binding| capability_for_binding(binding, discovery.config_generation.as_deref()))
        .collect::<BTreeSet<_>>();
    (capabilities.len() == 1)
        .then(|| capabilities.into_iter().next())
        .flatten()
}

/// Every canonical capability in one validated host discovery generation.
#[must_use]
pub fn usage_broker_capabilities(
    discovery: &ValidatedUsageDiscovery,
) -> Vec<UsageAccountCapability> {
    discovery
        .bindings
        .iter()
        .map(|binding| capability_for_binding(binding, discovery.config_generation.as_deref()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn usage_catalog_entries(discovery: &ValidatedUsageDiscovery) -> Vec<UsageCatalogEntry> {
    let mut source_ids = BTreeMap::<UsageAccountCapability, BTreeSet<String>>::new();
    for binding in &discovery.bindings {
        let capability = capability_for_binding(binding, discovery.config_generation.as_deref());
        source_ids
            .entry(capability)
            .or_default()
            .insert(binding.capability_id.clone());
    }
    source_ids
        .into_iter()
        .map(|(capability, source_ids)| {
            let revision_material = source_ids
                .iter()
                .map(|source_id| format!("{}:{source_id}", source_id.len()))
                .collect::<Vec<_>>()
                .join("|");
            UsageCatalogEntry {
                revision: jackin_core::account_key_hash(
                    "usage-catalog-entry-v2",
                    &revision_material,
                ),
                capability,
            }
        })
        .collect()
}

/// Small synchronous client. Each operation uses one bounded frame/connection.
///
/// A client also carries one monitoring screen's subscription set (see
/// `view`). Cloning forks that set: the clone starts with the same observed
/// generations but later (un)subscribes diverge.
#[derive(Debug)]
pub struct UsageBrokerClient {
    socket_path: PathBuf,
    build_id: String,
    subscriptions: Arc<Mutex<BTreeMap<UsageAccountCapability, u64>>>,
}

impl Clone for UsageBrokerClient {
    fn clone(&self) -> Self {
        let subscriptions = self
            .subscriptions
            .lock()
            .map(|subscriptions| subscriptions.clone())
            .unwrap_or_default();
        Self {
            socket_path: self.socket_path.clone(),
            build_id: self.build_id.clone(),
            subscriptions: Arc::new(Mutex::new(subscriptions)),
        }
    }
}

impl UsageBrokerClient {
    /// Attach to an already-running broker socket.
    #[must_use]
    pub fn at(socket_path: PathBuf, build_id: String) -> Self {
        Self {
            socket_path,
            build_id,
            subscriptions: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Attach to the current Capsule's per-container relay.
    #[must_use]
    pub fn scoped_relay() -> Self {
        Self::at(
            PathBuf::from(jackin_core::container_paths::USAGE_SOCK),
            env!("CARGO_PKG_VERSION").to_owned(),
        )
    }

    /// Read one account generation without provider work.
    pub fn current(
        &self,
        capability: UsageAccountCapability,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.execute(UsageBrokerOperation::Current { capability })
    }

    /// Read one exact account capability through a scoped relay.
    pub fn current_for_capability(
        &self,
        capability: UsageAccountCapability,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.execute(UsageBrokerOperation::CurrentForCapability { capability })
    }

    /// Request or join one account generation.
    pub fn refresh(
        &self,
        capability: UsageAccountCapability,
        observed_generation: u64,
        force: bool,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.execute(UsageBrokerOperation::Refresh {
            capability,
            observed_generation,
            force,
        })
    }

    /// Request or join one exact account capability through a scoped relay.
    pub fn refresh_for_capability(
        &self,
        capability: UsageAccountCapability,
        observed_generation: u64,
        force: bool,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.execute(UsageBrokerOperation::RefreshForCapability {
            capability,
            observed_generation,
            force,
        })
    }

    /// Wait for one named generation. This does not release broker ownership on timeout.
    pub fn join(
        &self,
        capability: UsageAccountCapability,
        generation: u64,
        timeout: Duration,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        self.execute(UsageBrokerOperation::Join {
            capability,
            generation,
            timeout_ms,
        })
    }

    /// Wait for one exact capability generation through a scoped relay.
    pub fn join_for_capability(
        &self,
        capability: UsageAccountCapability,
        generation: u64,
        timeout: Duration,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        self.execute(UsageBrokerOperation::JoinForCapability {
            capability,
            generation,
            timeout_ms,
        })
    }

    /// Execute one already-authorized broker operation.
    pub fn execute(
        &self,
        operation: UsageBrokerOperation,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.execute_with_scope(operation, None)
    }

    /// Execute one operation with the immutable launch proof supplied by the
    /// host relay. The caller cannot replace this with Capsule input because
    /// the relay owns the client invocation.
    pub fn execute_scoped(
        &self,
        operation: UsageBrokerOperation,
        scope: UsageCredentialScope,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.execute_with_scope(operation, Some(scope))
    }

    fn execute_with_scope(
        &self,
        operation: UsageBrokerOperation,
        launch_credential_scope: Option<UsageCredentialScope>,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        let request = UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: self.build_id.clone(),
            operation,
            launch_credential_scope,
        };
        let mut bytes = serde_json::to_vec(&request).map_err(|_| unavailable())?;
        if bytes.len() >= USAGE_BROKER_MAX_FRAME_BYTES {
            return Err(protocol_error());
        }
        bytes.push(b'\n');
        let mut stream = UnixStream::connect(&self.socket_path).map_err(|_| unavailable())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|_| unavailable())?;
        stream.write_all(&bytes).map_err(|_| unavailable())?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .map_err(|_| unavailable())?;
        let response = read_frame::<UsageBrokerResponse>(&mut stream)?;
        match response {
            UsageBrokerResponse::State { state } => Ok(*state),
            UsageBrokerResponse::Projection { .. } => Err(protocol_error()),
            UsageBrokerResponse::Error { error } => Err(error),
        }
    }

    /// Read the latest canonical publication without dispatching provider work.
    pub fn current_projection(&self) -> Result<UsageProjectionV1, UsageCoordinationError> {
        self.execute_projection(UsageBrokerOperation::CurrentProjection)
    }

    /// Request or join one broker-owned canonical projection refresh.
    pub fn request_refresh(
        &self,
        observed_projection_id: Option<String>,
        force: bool,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        self.execute_projection(UsageBrokerOperation::RequestRefresh {
            force,
            observed_projection_id,
        })
    }

    /// Join one immutable canonical publication without cancelling shared work.
    pub fn join_publication(
        &self,
        projection_id: String,
        timeout: Duration,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        self.execute_projection(UsageBrokerOperation::JoinPublication {
            projection_id,
            timeout_ms,
        })
    }

    /// Replace the host broker's current catalog and publish revocations.
    pub fn reconcile_catalog(
        &self,
        catalog_revision: String,
        entries: Vec<UsageCatalogEntry>,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        let expected_projection_id = self.current_projection()?.projection_id;
        self.reconcile_catalog_if_projection(
            Some(expected_projection_id),
            catalog_revision,
            entries,
        )
    }

    /// Replace the catalog against an explicitly observed publication lease.
    /// A stale caller is rejected by the broker rather than becoming the last
    /// writer.
    pub fn reconcile_catalog_if_projection(
        &self,
        expected_projection_id: Option<String>,
        catalog_revision: String,
        entries: Vec<UsageCatalogEntry>,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        self.execute_projection(UsageBrokerOperation::ReconcileCatalog {
            expected_projection_id,
            catalog_revision,
            entries,
        })
    }

    fn execute_projection(
        &self,
        operation: UsageBrokerOperation,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        let request = UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: self.build_id.clone(),
            operation,
            launch_credential_scope: None,
        };
        let mut bytes = serde_json::to_vec(&request).map_err(|_| unavailable())?;
        if bytes.len() >= USAGE_BROKER_MAX_FRAME_BYTES {
            return Err(protocol_error());
        }
        bytes.push(b'\n');
        let mut stream = UnixStream::connect(&self.socket_path).map_err(|_| unavailable())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|_| unavailable())?;
        stream.write_all(&bytes).map_err(|_| unavailable())?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .map_err(|_| unavailable())?;
        match read_frame::<UsageBrokerResponse>(&mut stream)? {
            UsageBrokerResponse::Projection { projection } => Ok(*projection),
            UsageBrokerResponse::Error { error } => Err(error),
            UsageBrokerResponse::State { .. } => Err(protocol_error()),
        }
    }
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

type LoadedProjection = (
    Arc<Mutex<UsageProjectionV1>>,
    Option<Vec<UsageCatalogEntry>>,
);

fn load_projection(config: &UsageBrokerConfig) -> Result<LoadedProjection, UsageCoordinationError> {
    let store = FileProjectionStateStore::under_data_dir(&config.data_dir);
    let loaded = match store.load() {
        Ok(loaded) => loaded,
        // v1 and invalid v2 envelopes are quarantined by the store. The
        // broker deliberately rebuilds an empty projection from current
        // discovery; unavailable state is not safe to overwrite.
        Err(crate::coordinator::StateStoreError::Corrupt) => None,
        Err(crate::coordinator::StateStoreError::Unavailable) => return Err(unavailable()),
    };
    let projection = loaded.as_ref().map_or_else(
        || empty_projection(&config.build_id),
        |envelope| envelope.projection.clone(),
    );
    let catalog = loaded.as_ref().map(|envelope| envelope.catalog.clone());
    let envelope = ProjectionStateEnvelope {
        schema_version: 2,
        catalog_revision: projection.discovery_revision.clone(),
        catalog: catalog.clone().unwrap_or_default(),
        broker_instance_id: projection.broker_instance_id.clone(),
        projection: projection.clone(),
        aliases: Vec::new(),
        retry_deadline_epoch: None,
        success_deadline_epoch: None,
    };
    store.store(&envelope).map_err(|_| unavailable())?;
    Ok((Arc::new(Mutex::new(projection)), catalog))
}

struct DiscoveryProviderExecutor {
    bindings: Mutex<BTreeMap<UsageAccountCapability, ValidatedCredentialBinding>>,
    scope: UsageDiscoveryScope,
    resolver: Arc<dyn ProviderCredentialEnvResolver>,
    probe_budget: Duration,
}

impl UsageProviderExecutor for DiscoveryProviderExecutor {
    fn authorize_credential_scope(
        &self,
        capability: &UsageAccountCapability,
        scope: &UsageCredentialScope,
    ) -> Result<(), UsageCoordinationError> {
        let binding = self
            .bindings
            .lock()
            .map_err(|_| unavailable())?
            .get(capability)
            .cloned()
            .ok_or_else(credential_scope_mismatch)?;
        let (key, material) = match &binding.source {
            ValidatedCredentialSource::Env {
                key,
                material: Some(material),
                ..
            } => (key, material),
            // Profile credentials are already materialized into the launch
            // auth tree and do not use the mutable env/op resolver lane.
            ValidatedCredentialSource::Profile(_) => return Ok(()),
            // An env binding without a source proof and a capability-only
            // binding have no host source to authorize.
            ValidatedCredentialSource::Env { .. } | ValidatedCredentialSource::Capability => {
                return Err(credential_scope_mismatch());
            }
        };
        let account_ids = binding
            .provenance
            .iter()
            .filter_map(|provenance| provenance.strip_prefix("account "));
        let account_ids = account_ids.map(str::to_owned).collect::<BTreeSet<_>>();
        credential_scope_matches(scope, &account_ids, &capability.surface_id, key, material)
            .then_some(())
            .ok_or_else(credential_scope_mismatch)
    }

    fn probe(&self, capability: &UsageAccountCapability, _generation: u64) -> ProviderProbeOutcome {
        // The coordinator only classifies elapsed time after a probe returns,
        // so the blocking provider call (child CLI/RPC, secret resolution)
        // runs under an explicit broker-side budget. Expiry completes the
        // generation through the normal failure path: last-good quota is
        // preserved and broker ownership is unaffected.
        let cached = self
            .bindings
            .lock()
            .ok()
            .and_then(|bindings| bindings.get(capability).cloned());
        let scope = self.scope.clone();
        let resolver = Arc::clone(&self.resolver);
        let task_capability = capability.clone();
        let outcome = probe::run_probe_with_budget(self.probe_budget, move || {
            let (binding, refreshed) = match cached {
                Some(binding) => (Some(binding), None),
                None => rediscover_bindings(&scope, resolver.as_ref(), &task_capability),
            };
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
                    && let Ok(mut bindings) = self.bindings.lock()
                {
                    *bindings = refreshed;
                }
                outcome
            }
            Err(_) => probe::probe_timeout_outcome(),
        }
    }

    fn reconcile_catalog(
        &self,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
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
}

fn rediscover_all_bindings(
    scope: &UsageDiscoveryScope,
    resolver: &dyn ProviderCredentialEnvResolver,
) -> Option<BTreeMap<UsageAccountCapability, ValidatedCredentialBinding>> {
    resolver.begin_manual_retry();
    discover_usage_sources(scope, resolver)
        .ok()
        .map(|catalog| validate_usage_sources(catalog, resolver))
        .map(|discovery| {
            discovery
                .bindings
                .into_iter()
                .map(|binding| {
                    (
                        capability_for_binding(&binding, discovery.config_generation.as_deref()),
                        binding,
                    )
                })
                .collect()
        })
}

fn rediscover_bindings(
    scope: &UsageDiscoveryScope,
    resolver: &dyn ProviderCredentialEnvResolver,
    capability: &UsageAccountCapability,
) -> (
    Option<ValidatedCredentialBinding>,
    Option<BTreeMap<UsageAccountCapability, ValidatedCredentialBinding>>,
) {
    let bindings = rediscover_all_bindings(scope, resolver);
    let binding = bindings
        .as_ref()
        .and_then(|bindings| bindings.get(capability).cloned());
    (binding, bindings)
}

fn refresh_binding_outcome(
    binding: &ValidatedCredentialBinding,
    resolver: &dyn ProviderCredentialEnvResolver,
) -> ProviderProbeOutcome {
    match refresh_credential_binding(binding, resolver) {
        ProviderCredentialRefreshOutcome::Snapshot(view) => provider_probe_outcome(*view),
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

fn provider_probe_outcome(
    view: jackin_protocol::control::FocusedUsageView,
) -> ProviderProbeOutcome {
    if let Some(error) = view
        .last_error
        .as_deref()
        .filter(|error| crate::usage::usage_error_is_rate_limited(error))
    {
        let retry_at_epoch = crate::usage::parse_retry_after_seconds(&error.to_ascii_lowercase())
            .map(|seconds| {
                chrono::Utc::now()
                    .timestamp()
                    .saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX))
            });
        return ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::RateLimited,
            message: "usage provider rate limit is active".to_owned(),
            retry_at_epoch,
        };
    }
    match view.status {
        UsageSnapshotStatus::NeedsSecret | UsageSnapshotStatus::NeedsLogin => {
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::NeedsSecret,
                message: "usage provider credentials require operator action".to_owned(),
                retry_at_epoch: None,
            }
        }
        UsageSnapshotStatus::Error
        | UsageSnapshotStatus::Unavailable
        | UsageSnapshotStatus::Stale => ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::ProviderUnavailable,
            message: "usage provider quota is unavailable".to_owned(),
            retry_at_epoch: None,
        },
        UsageSnapshotStatus::Unsupported => ProviderProbeOutcome::success(view),
        UsageSnapshotStatus::Fresh => ProviderProbeOutcome::success(view),
    }
}

/// Ensure one host broker backed by the validated Rust discovery generation.
pub fn ensure_usage_broker(
    config: UsageBrokerConfig,
    scope: UsageDiscoveryScope,
    discovery: ValidatedUsageDiscovery,
    _resolver: Arc<dyn ProviderCredentialEnvResolver>,
) -> Result<UsageBrokerHandle, UsageCoordinationError> {
    let mut scoped_capabilities = BTreeMap::<String, Vec<ScopedCapability>>::new();
    for binding in &discovery.bindings {
        let capability = capability_for_binding(binding, discovery.config_generation.as_deref());
        let requirement = forwarding_requirement(binding);
        for provenance in &binding.provenance {
            let scoped = scoped_capabilities.entry(provenance.clone()).or_default();
            scoped.push(ScopedCapability {
                capability: capability.clone(),
                requirement: requirement.clone(),
            });
        }
    }
    let capabilities = usage_broker_capabilities(&discovery);
    let catalog_revision = discovery
        .config_generation
        .clone()
        .unwrap_or_else(|| "empty".to_owned());
    let catalog = usage_catalog_entries(&discovery);
    if catalog.is_empty() {
        let probe_client = config.client();
        if !connect_probe(&probe_client) {
            return Ok(UsageBrokerHandle {
                client: probe_client,
                capabilities,
                catalog_lease: "no-catalog".to_owned(),
                scoped_capabilities,
            });
        }
        let expected_projection_id = probe_client.current_projection()?.projection_id;
        let projection = probe_client.reconcile_catalog_if_projection(
            Some(expected_projection_id),
            catalog_revision,
            catalog,
        )?;
        return Ok(UsageBrokerHandle {
            client: probe_client,
            capabilities,
            catalog_lease: projection.projection_id,
            scoped_capabilities,
        });
    }
    let client = ensure_usage_broker_process(config, &scope)?;
    let expected_projection_id = client.current_projection()?.projection_id;
    let projection = client.reconcile_catalog_if_projection(
        Some(expected_projection_id),
        catalog_revision,
        catalog,
    )?;
    Ok(UsageBrokerHandle {
        client,
        capabilities,
        catalog_lease: projection.projection_id,
        scoped_capabilities,
    })
}

/// Activate the independent broker executable and attach a client.
///
/// The caller never supplies an executor to this path. The sibling service
/// performs discovery and provider work in its own process, then survives the
/// activating client. Tests and legacy in-process seams use the hidden helper
/// below until their owning consumers migrate.
pub fn ensure_usage_broker_process(
    config: UsageBrokerConfig,
    scope: &UsageDiscoveryScope,
) -> Result<UsageBrokerClient, UsageCoordinationError> {
    let client = config.client();
    if connect_probe(&client) {
        return Ok(client);
    }
    let executable = config.service_executable.clone().ok_or_else(unavailable)?;
    let mut command = Command::new(executable);
    command
        .arg("--data-dir")
        .arg(&config.data_dir)
        .arg("--build-id")
        .arg(&config.build_id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match scope {
        UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home,
        } => {
            command
                .arg("--config-root")
                .arg(config_root)
                .arg("--operator-home")
                .arg(operator_home);
        }
        UsageDiscoveryScope::Capsule { .. } => return Err(unavailable()),
    }
    command.spawn().map_err(|_| unavailable())?;
    wait_for_leader(&client)?;
    Ok(client)
}

/// Run the process-owned service until its idle lease expires.
pub fn run_usage_broker_service(
    config: UsageBrokerConfig,
    scope: UsageDiscoveryScope,
    discovery: ValidatedUsageDiscovery,
    resolver: Arc<dyn ProviderCredentialEnvResolver>,
) -> Result<(), UsageCoordinationError> {
    let identity_metadata = publication_identity_metadata(&discovery);
    let catalog_revision = discovery
        .config_generation
        .clone()
        .unwrap_or_else(|| "empty".to_owned());
    let catalog = usage_catalog_entries(&discovery);
    let mut bindings = BTreeMap::new();
    for binding in discovery.bindings {
        bindings
            .entry(capability_for_binding(
                &binding,
                discovery.config_generation.as_deref(),
            ))
            .or_insert(binding);
    }
    let executor = Arc::new(DiscoveryProviderExecutor {
        bindings: Mutex::new(bindings),
        scope,
        resolver,
        probe_budget: config.coordinator.provider_timeout,
    });
    run_usage_broker_service_with_executor_and_metadata(
        config,
        executor,
        identity_metadata,
        Some((catalog_revision, catalog)),
    )
}

/// Process service seam used by the shipped broker binary and process tests.
pub fn run_usage_broker_service_with_executor(
    config: UsageBrokerConfig,
    executor: Arc<dyn UsageProviderExecutor>,
) -> Result<(), UsageCoordinationError> {
    run_usage_broker_service_with_executor_and_metadata(config, executor, BTreeMap::new(), None)
}

fn run_usage_broker_service_with_executor_and_metadata(
    config: UsageBrokerConfig,
    executor: Arc<dyn UsageProviderExecutor>,
    identity_metadata: BTreeMap<UsageAccountCapability, publish::AccountIdentityMetadata>,
    initial_catalog: Option<(String, Vec<UsageCatalogEntry>)>,
) -> Result<(), UsageCoordinationError> {
    let run_dir = secure_run_directory(&config.data_dir)?;
    let leader_path = run_dir.join(BROKER_LEADER);
    let Some(lease) = claim_leader(&leader_path, &config.build_id, config.lease_duration)? else {
        return Ok(());
    };
    let socket_path = config.socket_path();
    if socket_path.exists() {
        fs::remove_file(&socket_path).map_err(|_| unavailable())?;
    }
    let listener = UnixListener::bind(&socket_path).map_err(|_| unavailable())?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|_| unavailable())?;
    validate_owned_mode(&socket_path, 0o600)?;
    let store = Arc::new(FileAccountStateStore::under_data_dir(&config.data_dir));
    let (projection, persisted_catalog) = load_projection(&config)?;
    let previous_catalog = persisted_catalog.clone().unwrap_or_default();
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        executor,
        store,
        config.coordinator,
        previous_catalog.clone(),
    ));
    let publisher = publish::ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        FileProjectionStateStore::under_data_dir(&config.data_dir),
    )
    .with_identity_metadata(identity_metadata);
    let publisher = publisher.with_catalog(previous_catalog);
    if let Some((catalog_revision, catalog)) = initial_catalog {
        publisher.reconcile_catalog(catalog_revision, catalog, chrono::Utc::now().timestamp())?;
    }
    serve(ServeConfig {
        listener,
        coordinator,
        build_id: config.build_id.clone(),
        lease_path: leader_path,
        lease,
        policy: ServePolicy {
            idle_exit: config.idle_exit,
            lease_duration: config.lease_duration,
            lease_renewal: config.lease_renewal,
        },
        publisher,
    });
    Ok(())
}

/// Test/runtime seam that preserves the same process election and transport.
#[doc(hidden)]
pub fn ensure_usage_broker_with_executor(
    config: UsageBrokerConfig,
    executor: Arc<dyn UsageProviderExecutor>,
) -> Result<UsageBrokerClient, UsageCoordinationError> {
    let socket_path = config.socket_path();
    let client = UsageBrokerClient::at(socket_path.clone(), config.build_id.clone());
    if connect_probe(&client) {
        return Ok(client);
    }

    let run_dir = secure_run_directory(&config.data_dir)?;
    let leader_path = run_dir.join(BROKER_LEADER);
    let Some(lease) = claim_leader(&leader_path, &config.build_id, config.lease_duration)? else {
        wait_for_leader(&client)?;
        return Ok(client);
    };

    if socket_path.exists() {
        fs::remove_file(&socket_path).map_err(|_| unavailable())?;
    }
    let listener = UnixListener::bind(&socket_path).map_err(|_| unavailable())?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|_| unavailable())?;
    validate_owned_mode(&socket_path, 0o600)?;
    let store = Arc::new(FileAccountStateStore::under_data_dir(&config.data_dir));
    let (projection, persisted_catalog) = load_projection(&config)?;
    let coordinator = match persisted_catalog.clone() {
        Some(catalog) => Arc::new(UsageCoordinator::with_catalog(
            executor,
            store,
            config.coordinator,
            catalog,
        )),
        None => Arc::new(UsageCoordinator::new(executor, store, config.coordinator)),
    };
    let build_id = config.build_id.clone();
    let idle_exit = config.idle_exit;
    let lease_duration = config.lease_duration;
    let lease_renewal = config.lease_renewal;
    let lease_path = leader_path.clone();
    let publisher = publish::ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        FileProjectionStateStore::under_data_dir(&config.data_dir),
    );
    let publisher = match persisted_catalog {
        Some(catalog) => publisher.with_catalog(catalog),
        None => publisher,
    };
    jackin_telemetry::spawn::thread_joined_named("usage-broker".to_owned(), move || {
        serve(ServeConfig {
            listener,
            coordinator,
            build_id,
            lease_path,
            lease,
            policy: ServePolicy {
                idle_exit,
                lease_duration,
                lease_renewal,
            },
            publisher,
        });
    })
    .map_err(|_| unavailable())?;
    wait_for_leader(&client)?;
    Ok(client)
}

mod probe;
mod publish;
mod view;
mod waits;

struct ServeConfig {
    listener: UnixListener,
    coordinator: Arc<UsageCoordinator>,
    build_id: String,
    lease_path: PathBuf,
    lease: BrokerLease,
    policy: ServePolicy,
    publisher: publish::ProjectionPublisher,
}

fn serve(config: ServeConfig) {
    let ServeConfig {
        listener,
        coordinator,
        build_id,
        lease_path,
        mut lease,
        policy,
        publisher,
    } = config;
    let (connections, receiver) = mpsc::sync_channel(BROKER_CONNECTION_QUEUE);
    let receiver = Arc::new(Mutex::new(receiver));
    let build_id = Arc::<str>::from(build_id.as_str());
    let wait_pool = waits::WaitPool::new(
        Arc::clone(&coordinator),
        Arc::clone(&build_id),
        publisher.clone(),
    );
    let wait_pool = Arc::new(wait_pool);
    let mut workers = Vec::with_capacity(BROKER_CONNECTION_WORKERS);
    for index in 0..BROKER_CONNECTION_WORKERS {
        let receiver = Arc::clone(&receiver);
        let coordinator = Arc::clone(&coordinator);
        let build_id = Arc::clone(&build_id);
        let publisher = publisher.clone();
        let wait_pool = Arc::clone(&wait_pool);
        let worker = jackin_telemetry::spawn::thread_joined_named(
            format!("usage-broker-connection-{index}"),
            move || loop {
                let stream = {
                    let Ok(receiver) = receiver.lock() else {
                        return;
                    };
                    receiver.recv()
                };
                let Ok(stream) = stream else {
                    return;
                };
                handle_stream(stream, &coordinator, &build_id, &publisher, &wait_pool);
            },
        );
        match worker {
            Ok(worker) => workers.push(worker),
            Err(_) => break,
        }
    }
    if workers.is_empty() {
        return;
    }
    if listener.set_nonblocking(true).is_err() {
        drop(connections);
        for worker in workers {
            drop(worker.join());
        }
        return;
    }
    // Incremental publisher: while any generation is active, merge completed
    // accounts into the canonical projection as they finish. One stalled
    // account never blocks healthy accounts; dispatch-path publishing covers
    // promptness when this ticker cannot spawn.
    let publisher_shutdown = Arc::new(AtomicBool::new(false));
    let ticker = {
        let publisher = publisher.clone();
        let ticker_coordinator = Arc::clone(&coordinator);
        let shutdown = Arc::clone(&publisher_shutdown);
        jackin_telemetry::spawn::thread_joined_named(
            "usage-broker-publisher".to_owned(),
            move || {
                while !shutdown.load(Ordering::Relaxed) {
                    std::thread::park_timeout(PUBLISH_TICK);
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                    if !ticker_coordinator.is_idle() {
                        publisher.publish_due(chrono::Utc::now().timestamp());
                    }
                }
            },
        )
        .ok()
    };
    let started = Instant::now();
    let mut last_activity = started;
    let mut last_renewal = started;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                last_activity = Instant::now();
                match connections.try_send(stream) {
                    Ok(()) => {}
                    Err(
                        TrySendError::Full(mut stream) | TrySendError::Disconnected(mut stream),
                    ) => {
                        write_response(
                            &mut stream,
                            UsageBrokerResponse::Error {
                                error: unavailable(),
                            },
                        );
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                let now = Instant::now();
                if now.duration_since(last_renewal) >= policy.lease_renewal {
                    if !renew_lease(&lease_path, &mut lease, policy.lease_duration) {
                        break;
                    }
                    last_renewal = now;
                }
                if now.duration_since(last_activity) >= policy.idle_exit && coordinator.is_idle() {
                    break;
                }
                std::thread::park_timeout(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
    drop(connections);
    for worker in workers {
        drop(worker.join());
    }
    publisher_shutdown.store(true, Ordering::Relaxed);
    if let Some(ticker) = ticker {
        drop(ticker.join());
    }
    let _ignored = remove_lease(&lease_path, &lease);
    let _ignored = fs::remove_file(lease_path.with_file_name(BROKER_SOCKET));
}

fn handle_stream(
    mut stream: UnixStream,
    coordinator: &UsageCoordinator,
    build_id: &str,
    publisher: &publish::ProjectionPublisher,
    waits: &waits::WaitPool,
) {
    let response = match read_request(&mut stream) {
        Ok(request) if waits::is_wait(&request.operation) => {
            waits.enqueue(stream, request);
            return;
        }
        Ok(request) => dispatch(coordinator, request, build_id, publisher),
        Err(error) => UsageBrokerResponse::Error { error },
    };
    write_response(&mut stream, response);
}

fn write_response(stream: &mut UnixStream, response: UsageBrokerResponse) {
    if let Ok(mut bytes) = serde_json::to_vec(&response)
        && bytes.len() < USAGE_BROKER_MAX_FRAME_BYTES
    {
        bytes.push(b'\n');
        write_with_deadline(stream, &bytes, Duration::from_secs(1));
    }
}

fn write_with_deadline(stream: &mut UnixStream, mut bytes: &[u8], timeout: Duration) {
    if stream.set_nonblocking(true).is_err() {
        return;
    }
    let deadline = Instant::now() + timeout;
    while !bytes.is_empty() && Instant::now() < deadline {
        match stream.write(bytes) {
            Ok(0) => return,
            Ok(written) => bytes = &bytes[written..],
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::park_timeout(Duration::from_millis(1));
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

fn read_request(stream: &mut UnixStream) -> Result<UsageBrokerRequest, UsageCoordinationError> {
    stream.set_nonblocking(true).map_err(|_| unavailable())?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];
    while Instant::now() < deadline {
        match stream.read(&mut chunk) {
            Ok(0) => return Err(protocol_error()),
            Ok(read) => {
                bytes.extend_from_slice(&chunk[..read]);
                if bytes.len() > USAGE_BROKER_MAX_FRAME_BYTES {
                    return Err(protocol_error());
                }
                if bytes.last() == Some(&b'\n') {
                    return serde_json::from_slice(&bytes).map_err(|_| protocol_error());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::park_timeout(Duration::from_millis(1));
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err(unavailable()),
        }
    }
    Err(unavailable())
}

fn dispatch(
    coordinator: &UsageCoordinator,
    request: UsageBrokerRequest,
    build_id: &str,
    publisher: &publish::ProjectionPublisher,
) -> UsageBrokerResponse {
    let UsageBrokerRequest {
        protocol_version,
        build_id: request_build_id,
        operation,
        launch_credential_scope,
    } = request;
    if protocol_version != USAGE_BROKER_PROTOCOL_VERSION || request_build_id != build_id {
        return UsageBrokerResponse::Error {
            error: protocol_error(),
        };
    }
    if launch_credential_scope.is_some()
        && !matches!(
            &operation,
            UsageBrokerOperation::Current { .. }
                | UsageBrokerOperation::Refresh { .. }
                | UsageBrokerOperation::Join { .. }
        )
    {
        return UsageBrokerResponse::Error {
            error: UsageCoordinationError {
                kind: UsageCoordinationErrorKind::Unauthorized,
                message: "launch credential scope requires an account operation".to_owned(),
            },
        };
    }
    match &operation {
        UsageBrokerOperation::ReconcileCatalog {
            expected_projection_id,
            catalog_revision,
            entries,
        } => {
            return match publisher.reconcile_catalog_if_projection(
                expected_projection_id.as_deref(),
                catalog_revision.clone(),
                entries.clone(),
                chrono::Utc::now().timestamp(),
            ) {
                Ok(projection) => UsageBrokerResponse::Projection {
                    projection: Box::new(projection),
                },
                Err(error) => UsageBrokerResponse::Error { error },
            };
        }
        UsageBrokerOperation::CurrentProjection => return read_projection(publisher),
        UsageBrokerOperation::RequestRefresh {
            force,
            observed_projection_id: _,
        } => return refresh_projection(coordinator, publisher, *force),
        UsageBrokerOperation::JoinPublication {
            projection_id,
            timeout_ms,
        } => return join_publication(publisher, projection_id, *timeout_ms),
        UsageBrokerOperation::CurrentProjectionForSurface
        | UsageBrokerOperation::RequestRefreshForSurface { .. }
        | UsageBrokerOperation::JoinPublicationForSurface { .. } => {
            return UsageBrokerResponse::Error {
                error: UsageCoordinationError {
                    kind: UsageCoordinationErrorKind::Unauthorized,
                    message: "scoped projection operation requires a container relay".to_owned(),
                },
            };
        }
        _ => {}
    }
    let now = chrono::Utc::now().timestamp();
    let result = match operation {
        UsageBrokerOperation::ReconcileCatalog { .. } => Err(protocol_error()),
        UsageBrokerOperation::CurrentProjection
        | UsageBrokerOperation::RequestRefresh { .. }
        | UsageBrokerOperation::JoinPublication { .. }
        | UsageBrokerOperation::CurrentProjectionForSurface
        | UsageBrokerOperation::RequestRefreshForSurface { .. }
        | UsageBrokerOperation::JoinPublicationForSurface { .. } => Err(protocol_error()),
        UsageBrokerOperation::CurrentForCapability { .. }
        | UsageBrokerOperation::RefreshForCapability { .. }
        | UsageBrokerOperation::JoinForCapability { .. } => Err(UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unauthorized,
            message: "scoped usage operation requires a container relay".to_owned(),
        }),
        UsageBrokerOperation::Current { capability } => {
            if let Some(scope) = launch_credential_scope.as_ref()
                && let Err(error) = coordinator.authorize_credential_scope(&capability, scope)
            {
                return UsageBrokerResponse::Error { error };
            }
            publisher.observe(&capability);
            let result = coordinator.current(&capability, now);
            publisher.publish_due(now);
            result
        }
        UsageBrokerOperation::Refresh {
            capability,
            observed_generation,
            force,
        } => {
            if let Some(scope) = launch_credential_scope.as_ref()
                && let Err(error) = coordinator.authorize_credential_scope(&capability, scope)
            {
                return UsageBrokerResponse::Error { error };
            }
            publisher.observe(&capability);
            let result = coordinator.request_refresh(&capability, observed_generation, force, now);
            publisher.publish_due(now);
            result
        }
        UsageBrokerOperation::Join {
            capability,
            generation,
            timeout_ms,
        } => {
            if let Some(scope) = launch_credential_scope.as_ref()
                && let Err(error) = coordinator.authorize_credential_scope(&capability, scope)
            {
                return UsageBrokerResponse::Error { error };
            }
            publisher.observe(&capability);
            let result = coordinator.join_generation(
                &capability,
                generation,
                Duration::from_millis(timeout_ms.min(30_000)),
                now,
            );
            publisher.publish_due(now);
            result
        }
    };
    match result {
        Ok(state) => UsageBrokerResponse::State {
            state: Box::new(state),
        },
        Err(error) => UsageBrokerResponse::Error { error },
    }
}

fn read_projection(publisher: &publish::ProjectionPublisher) -> UsageBrokerResponse {
    match publisher.current_projection() {
        Ok(projection) => UsageBrokerResponse::Projection {
            projection: Box::new(projection),
        },
        Err(error) => UsageBrokerResponse::Error { error },
    }
}

/// Request due observations for every observed account and return the latest
/// publication. Each account runs its normal due check: still-fresh data is
/// reused, active work is joined, and retry deadlines always win. `force` is
/// honored only as the coordinator honors it — an explicit operator refresh
/// bypasses the success cooldown but never retry or rate-limit deadlines.
fn refresh_projection(
    coordinator: &UsageCoordinator,
    publisher: &publish::ProjectionPublisher,
    force: bool,
) -> UsageBrokerResponse {
    let now = chrono::Utc::now().timestamp();
    let known = publisher.known_capabilities();
    if !known.is_empty() {
        let mut requests = Vec::with_capacity(known.len());
        for capability in &known {
            // A read failure resolves to generation 0, which can only adopt
            // the current winner — never force a duplicate generation.
            let observed = coordinator
                .current(capability, now)
                .map_or(0, |view| view.generation);
            requests.push((capability.clone(), observed));
        }
        let _ignored = coordinator.request_refresh_all(requests, force, now);
        publisher.publish_due(now);
    }
    read_projection(publisher)
}

/// Wait until one named publication settles or is superseded.
///
/// A newer publication means the requested one is terminal history and is
/// returned immediately. Waiting only happens while the requested publication
/// is current and still refreshing. Expiry reports `WaitTimeout` without
/// touching broker ownership: generations always run to terminal.
fn join_publication(
    publisher: &publish::ProjectionPublisher,
    projection_id: &str,
    timeout_ms: u64,
) -> UsageBrokerResponse {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.min(30_000));
    loop {
        let current = match publisher.current_projection() {
            Ok(current) => current,
            Err(error) => return UsageBrokerResponse::Error { error },
        };
        if current.projection_id != projection_id
            || current.refresh_state != UsageProjectionRefreshStateV1::Refreshing
        {
            return UsageBrokerResponse::Projection {
                projection: Box::new(current),
            };
        }
        publisher.publish_due(chrono::Utc::now().timestamp());
        if Instant::now() >= deadline {
            return UsageBrokerResponse::Error {
                error: UsageCoordinationError {
                    kind: UsageCoordinationErrorKind::WaitTimeout,
                    message: "usage projection publication is still refreshing".to_owned(),
                },
            };
        }
        std::thread::park_timeout(Duration::from_millis(50));
    }
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
) -> Result<Option<BrokerLease>, UsageCoordinationError> {
    let lease = BrokerLease::new(build_id);
    match open(
        path,
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW,
        Mode::from_bits_truncate(0o600),
    ) {
        Ok(fd) => {
            let mut file = File::from(fd);
            let bytes = serde_json::to_vec(&lease).map_err(|_| unavailable())?;
            file.write_all(&bytes).map_err(|_| unavailable())?;
            file.sync_all().map_err(|_| unavailable())?;
            Ok(Some(lease))
        }
        Err(nix::errno::Errno::EEXIST) => {
            validate_owned_mode(path, 0o600)?;
            let bytes = fs::read(path).map_err(|_| unavailable())?;
            let existing = serde_json::from_slice::<BrokerLease>(&bytes).ok();
            let now = chrono::Utc::now().timestamp();
            if let Some(existing) = existing {
                if existing.protocol_version != USAGE_BROKER_PROTOCOL_VERSION
                    || existing.build_id != build_id
                {
                    // A healthy incompatible endpoint is never replaced by an
                    // activator; the client will receive protocol_mismatch.
                    return Ok(None);
                }
                if now.saturating_sub(existing.renewed_at_epoch)
                    < i64::try_from(lease_duration.as_secs()).unwrap_or(i64::MAX)
                {
                    return Ok(None);
                }
            } else {
                // Preserve compatibility with pre-lease state only when its PID
                // is demonstrably gone; a malformed live lease fails closed.
                let pid = String::from_utf8_lossy(&bytes).trim().parse::<i32>().ok();
                if pid.is_none_or(|pid| kill(Pid::from_raw(pid), None).is_ok()) {
                    return Ok(None);
                }
            }
            fs::remove_file(path).map_err(|_| unavailable())?;
            claim_leader(path, build_id, lease_duration)
        }
        Err(_) => Err(unavailable()),
    }
}

fn renew_lease(path: &Path, lease: &mut BrokerLease, lease_duration: Duration) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let Ok(mut current) = serde_json::from_slice::<BrokerLease>(&bytes) else {
        return false;
    };
    if current.instance_id != lease.instance_id {
        return false;
    }
    let now = chrono::Utc::now().timestamp();
    if now.saturating_sub(lease.renewed_at_epoch)
        > i64::try_from(lease_duration.as_secs()).unwrap_or(i64::MAX)
    {
        return false;
    }
    current.renewed_at_epoch = now;
    let Some(parent) = path.parent() else {
        return false;
    };
    let Some(temporary) = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!(".{name}.renew.tmp"))
    else {
        return false;
    };
    let Ok(directory) = open(
        parent,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    ) else {
        return false;
    };
    let directory = File::from(directory);
    let Ok(bytes) = serde_json::to_vec(&current) else {
        return false;
    };
    let Ok(fd) = openat(
        &directory,
        temporary.as_str(),
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW,
        Mode::from_bits_truncate(0o600),
    ) else {
        return false;
    };
    let mut file = File::from(fd);
    if file.write_all(&bytes).is_err() || file.sync_all().is_err() {
        let _ignored = unlinkat(&directory, temporary.as_str(), UnlinkatFlags::NoRemoveDir);
        return false;
    }
    let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if renameat(&directory, temporary.as_str(), &directory, filename).is_err() {
        let _ignored = unlinkat(&directory, temporary.as_str(), UnlinkatFlags::NoRemoveDir);
        return false;
    }
    if fsync(&directory).is_err() {
        return false;
    }
    lease.renewed_at_epoch = now;
    true
}

fn remove_lease(path: &Path, lease: &BrokerLease) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let Ok(current) = serde_json::from_slice::<BrokerLease>(&bytes) else {
        return false;
    };
    current.instance_id == lease.instance_id && fs::remove_file(path).is_ok()
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
    UnixStream::connect(&client.socket_path).is_ok()
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
    let subject = match catalog_revision {
        Some(revision) => format!(
            "usage-capability-v2:catalog-revision:{}:{revision}:subject:{}:{subject}",
            revision.len(),
            subject.len()
        ),
        None => subject,
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
            Some(
                CanonicalAccountSubject::ProviderStableHandle(_)
                | CanonicalAccountSubject::SourceCapability(_),
            ) => {
                // Wire V1 has no separate source-scoped kind; both are stable
                // non-secret handles and never carry the capability itself.
                UsageIdentityKindV1::ProviderStableHandle
            }
            None => UsageIdentityKindV1::ProviderAccountId,
        };
        let entry = evidence
            .entry(capability)
            .or_insert_with(|| (identity_kind, BTreeSet::new()));
        // A provider-issued id is stronger evidence than a stable display
        // handle if malformed input ever aliases them to one capability.
        if identity_kind == UsageIdentityKindV1::ProviderAccountId {
            entry.0 = UsageIdentityKindV1::ProviderAccountId;
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

fn unavailable() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::Unavailable,
        message: "usage broker is unavailable".to_owned(),
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

#[cfg(test)]
mod tests;
