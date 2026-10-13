// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Host runtime open configuration and paths.

use super::{UsageBrokerClient, UsageDiscoveryScope};

use jackin_core::account_key_hash;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageGenerationView,
};

/// Relative data-dir subtree for menu-bar durable state.
pub const HOST_USAGE_STATE_REL: &str = "usage-menu-bar";
pub use jackin_usage_host_presentation::SELECTED_ACCOUNT_UNAVAILABLE_NOTICE;

pub(crate) static CANONICAL_INSTANCE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn canonical_instance_id() -> String {
    let epoch_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = CANONICAL_INSTANCE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    account_key_hash(
        "usage-broker-instance-v1",
        &format!("{epoch_nanos}:{sequence}"),
    )
}

/// Open configuration for the host runtime.
#[derive(Debug, Clone)]
pub struct HostRuntimeConfig {
    /// jackin data dir (`~/.jackin/data` or test root).
    pub data_dir: PathBuf,
    /// Minimum refresh interval floor (seconds). Clamped to ≥ 60.
    pub refresh_floor_secs: u64,
    /// Initially enabled surface ids; empty → all host surfaces.
    pub enabled_surface_ids: Vec<String>,
    /// Whether this runtime may dispatch live provider probes. `Disabled` is
    /// used by the isolated launch smoke test so an accidental refresh cannot
    /// reach any credential/file/env/CLI/network/Keychain resolution.
    pub probe_policy: HostProbePolicy,
    /// Account-discovery authority for this runtime.
    pub discovery_scope: UsageDiscoveryScope,
}

/// Whether a host runtime may dispatch live provider probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HostProbePolicy {
    /// Normal operation: refreshes dispatch provider probes.
    #[default]
    Live,
    /// Smoke/defense-in-depth: refresh is a no-probe no-op and never due.
    Disabled,
}

impl HostRuntimeConfig {
    /// Default host layout under `data_dir` (live probes).
    #[must_use]
    pub fn under_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            refresh_floor_secs: 300,
            enabled_surface_ids: Vec::new(),
            probe_policy: HostProbePolicy::Live,
            discovery_scope: UsageDiscoveryScope::Capsule {
                forwarded_accounts: Vec::new(),
            },
        }
    }
}

/// Snapshot store path under the host data dir.
#[must_use]
pub fn host_snapshot_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join(HOST_USAGE_STATE_REL).join("snapshots.db")
}

/// Materialized accounts JSON path under the host data dir.
#[must_use]
pub fn host_accounts_path(data_dir: &Path) -> PathBuf {
    data_dir.join(HOST_USAGE_STATE_REL).join("accounts.json")
}

/// Bounded batch broker read for console usage screens.
///
/// Issues one refresh request per unique capability and returns the broker's
/// immediate answer for each: cached or last-good quota plus the live phase.
/// This performs no blocking join — one slow provider's probe runs
/// broker-side and never delays the other accounts' reads or the calling
/// thread. Freshness arrives over subsequent heartbeat polls, which re-request
/// (and join) through the same path.
///
/// Per-account failures are reported alongside successes, never as a batch
/// abort. Pass `force: true` only for an explicit operator refresh: it
/// bypasses the broker success cadence, while shared rate-limit/`Retry-After`
/// deadlines are still honored broker-side and active generations are joined
/// rather than duplicated.
#[must_use]
pub fn request_usage_batch(
    client: &UsageBrokerClient,
    capabilities: impl IntoIterator<Item = UsageAccountCapability>,
    force: bool,
) -> Vec<(
    UsageAccountCapability,
    Result<UsageGenerationView, UsageCoordinationError>,
)> {
    let mut results = Vec::new();
    for capability in capabilities
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let observed = client
            .current(capability.clone())
            .map_or(0, |view| view.generation);
        let result = client.refresh(capability.clone(), observed, force);
        results.push((capability, result));
    }
    results
}
