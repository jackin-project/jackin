// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule-free host usage projection for the macOS menu-bar app and CLI.
//!
//! Provider work and shared state are owned by the host usage broker. This
//! runtime holds presentation state only.

mod accounts;
mod broker;
mod config;
mod credential_resolver;
mod desktop;
mod discovery;
mod event_log;
mod inventory;
mod lifecycle;
mod projection;
mod render;
mod selection;
mod snapshots;
mod staging;
mod status_bar;
mod surface_control;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::atomic::Ordering;
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;

#[cfg(test)]
use jackin_core::Agent;
use jackin_protocol::control::FocusedUsageView;
#[cfg(test)]
use jackin_protocol::usage_broker::UsageCoordinationError;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageGenerationView, UsageProjectionV1, UsageRefreshPhase,
};

use jackin_usage_provider_core::{UsageCache, UsageFormatPrefs};

pub use accounts::{
    AccountLifecycle, AccountProvenance, CanonicalAccountIdentity, CanonicalAccountSubject,
    HostAccountDescriptor, account_key_for_view, canonical_account_id_for_view, min_remaining,
    short_account_identity,
};
pub use broker::{
    ForwardedUsageSources, UsageBrokerClient, UsageBrokerConfig, UsageBrokerHandle,
    ensure_usage_broker, ensure_usage_broker_process, ensure_usage_broker_with_executor,
    forwarded_usage_capabilities, run_usage_broker_service, run_usage_broker_service_with_executor,
    usage_broker_capabilities, usage_capability_for_selected_account,
    usage_capability_for_selected_account_with_sources,
};
pub(crate) use config::canonical_instance_id;
pub use config::{
    HOST_USAGE_STATE_REL, HostProbePolicy, HostRuntimeConfig, SELECTED_ACCOUNT_UNAVAILABLE_NOTICE,
    host_accounts_path, host_snapshot_store_path, request_usage_batch,
};
pub use credential_resolver::{
    CachedProviderCredentialResolver, ProviderCredentialSecretOutcome,
    ProviderCredentialSecretResolution, ProviderCredentialSecretSource,
};
pub use desktop::{
    HostDesktopInventory, HostDesktopProjection, HostDesktopProviderGroup,
    HostDesktopProviderProjection, HostDesktopProviderState, HostSelectedAccountRoute,
};
pub use discovery::{
    DiscoveredAccountDescriptor, ForwardedUsageAccount, HostCredentialRootRow,
    OpaqueCredentialHandle, ProviderCredentialEnvOutcome, ProviderCredentialEnvResolution,
    ProviderCredentialEnvResolver, ProviderCredentialIdentityOutcome,
    ProviderCredentialRefreshOutcome, ProviderCredentialSourceMaterial, UsageCredentialKind,
    UsageDiscoveryCatalog, UsageDiscoveryDiagnostic, UsageDiscoveryIssue, UsageDiscoveryScope,
    UsageSourceCandidateDescriptor, ValidatedUsageDiscovery, discover_usage_sources,
    host_credential_root_matrix, validate_usage_sources,
};
pub use jackin_usage_host_presentation::{HostEventBatch, HostUsageEvent};
pub use jackin_usage_host_presentation::{HostOverviewRow, HostProviderGlanceRow};
pub use jackin_usage_host_presentation::{HostSurfaceDescriptor, HostSurfaceId};
pub(crate) use jackin_usage_host_presentation::{MAX_EVENT_BATCH, MAX_EVENT_LOG};
pub use projection::{NormalizedUsageDestination, UsageDestination, normalize_destination};
pub use render::STATUS_BAR_MAX_CHIPS;
pub(crate) use render::{
    DrivingBucket, account_descriptor, build_provider_glance_row, drive_label_prefix,
    driving_bucket_from_view, glance_bucket, selected_account_unavailable_view,
    status_bar_rank_key, view_is_auto_detected, worst_severity_label,
};
pub use staging::StagedUsageDiscovery;
pub(crate) use staging::{discovered_account_keys, enabled_surface_ids};

/// Capsule-free host usage runtime.
#[derive(Debug, Clone)]
pub struct HostUsageRuntime {
    cache: UsageCache,
    enabled: HashSet<String>,
    events: VecDeque<HostUsageEvent>,
    next_seq: u64,
    refresh_floor_secs: u64,
    /// Last time a network-bearing refresh completed (floor gate).
    last_refresh: Option<Instant>,
    /// Presentation-time format prefs (not persisted).
    format_prefs: UsageFormatPrefs,
    open: bool,
    /// Absolute jackin data dir (for snapshot store + selected-accounts prefs).
    data_dir: Option<PathBuf>,
    /// Selected account key per surface id (persisted).
    selected_accounts: HashMap<String, String>,
    /// Whether live probes may dispatch (smoke mode disables them).
    probe_policy: HostProbePolicy,
    /// Provider ids currently auto-detected for the Desktop glance list.
    /// Runtime-only (never persisted); holds ids, never display strings.
    desktop_detected_surfaces: HashSet<String>,
    /// Last completed current-membership discovery generation.
    discovery: Option<ValidatedUsageDiscovery>,
    /// Monotonic local freshness fence for staged discovery commits.
    discovery_generation: u64,
    /// Last quota snapshots fetched from explicit current discovery sources.
    discovered_views: BTreeMap<(HostSurfaceId, String), FocusedUsageView>,
    /// Explicit source state without authenticated account identity yet.
    discovered_provider_views: BTreeMap<HostSurfaceId, FocusedUsageView>,
    /// Scope retained for explicit manual reconciliation only.
    discovery_scope: Option<UsageDiscoveryScope>,
    /// Broker generation phase per canonical account.
    broker_phases: BTreeMap<UsageAccountCapability, UsageRefreshPhase>,
    /// Complete broker state retained for canonical freshness and issues.
    broker_generations: BTreeMap<UsageAccountCapability, UsageGenerationView>,
    canonical_instance_id: String,
    canonical_content_id: Option<String>,
    canonical_projection_cache: Option<UsageProjectionV1>,
    canonical_identity_graph: accounts::CanonicalIdentityGraph,
}

#[cfg(test)]
mod tests;
