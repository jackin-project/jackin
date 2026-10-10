// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Broker-owned host usage discovery and credential-free publication clients.

mod accounts;
mod broker;
mod credential_resolver;
mod discovery;
mod model;
mod projection;
mod projection_runtime;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageGenerationView,
};

pub use crate::usage::{
    ClaudeKeychainPolicyError, ClaudeKeychainRead, ClaudeUnattendedKeychainGuard,
    prepare_claude_keychain_auth, read_claude_keychain_item, unattended_keychain_guard,
};
pub use accounts::{CanonicalAccountIdentity, CanonicalAccountSubject};
pub use broker::{
    ForwardedUsageSources, UsageBrokerClient, UsageBrokerConfig, UsageBrokerForegroundReady,
    ensure_usage_broker_process, ensure_usage_broker_with_executor, parse_statusline,
    run_usage_broker_foreground_bootstrap, run_usage_broker_service,
    run_usage_broker_service_with_executor,
};
pub use credential_resolver::{
    CachedProviderCredentialResolver, ProviderCredentialSecretOutcome,
    ProviderCredentialSecretResolution, ProviderCredentialSecretSource,
};
pub use discovery::{
    DiscoveredAccountDescriptor, ForwardedUsageAccount, HostCredentialRootRow,
    OpaqueCredentialHandle, ProviderCredentialEnvOutcome, ProviderCredentialEnvResolution,
    ProviderCredentialEnvResolver, ProviderCredentialIdentityOutcome,
    ProviderCredentialRefreshOutcome, ProviderCredentialSourceMaterial, UsageCredentialKind,
    UsageDiscoveryCatalog, UsageDiscoveryDiagnostic, UsageDiscoveryIssue, UsageDiscoveryScope,
    UsageDiscoveryUnresolvedSource, UsageSourceCandidateDescriptor, ValidatedUsageDiscovery,
    discover_usage_sources, host_credential_root_matrix, validate_usage_sources,
};
pub use model::HostSurfaceId;
pub use projection::{NormalizedUsageDestination, UsageDestination, normalize_destination};
pub use projection_runtime::{
    HostUsageProjectionAccountPresentation, HostUsageProjectionConfig,
    HostUsageProjectionProviderPresentation, HostUsageProjectionRuntime,
    HostUsageProjectionSelectedAccount,
};

/// Relative data-dir subtree for native projection selection state.
pub const HOST_USAGE_STATE_REL: &str = "usage-menu-bar";

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

#[cfg(test)]
mod tests;
