// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Binding rediscovery and probe outcomes.

use std::collections::BTreeMap;

use jackin_protocol::control::UsageSnapshotStatus;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind,
};

use crate::coordinator::ProviderProbeOutcome;

use super::super::discovery::{
    ProviderCredentialEnvResolver, ProviderCredentialRefreshOutcome, ValidatedCredentialBinding,
    discover_usage_sources, refresh_credential_binding, validate_usage_sources,
};
use super::super::{UsageDiscoveryScope, ValidatedUsageDiscovery};
use super::{capability_for_binding, catalog_discovery_mismatch, usage_catalog_entries};

pub(crate) fn rediscover_discovery(
    scope: &UsageDiscoveryScope,
    resolver: &dyn ProviderCredentialEnvResolver,
) -> Option<ValidatedUsageDiscovery> {
    discover_usage_sources(scope, resolver)
        .ok()
        .map(|catalog| validate_usage_sources(catalog, resolver))
}

pub(crate) fn rediscover_all_bindings(
    scope: &UsageDiscoveryScope,
    resolver: &dyn ProviderCredentialEnvResolver,
) -> Option<BTreeMap<UsageAccountCapability, Vec<ValidatedCredentialBinding>>> {
    rediscover_discovery(scope, resolver).map(|discovery| grouped_bindings(&discovery))
}

pub(crate) type RediscoveredBindings = (
    Option<Vec<ValidatedCredentialBinding>>,
    Option<BTreeMap<UsageAccountCapability, Vec<ValidatedCredentialBinding>>>,
);

pub(crate) fn rediscover_bindings(
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

pub(crate) fn grouped_bindings(
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

pub(crate) fn catalog_entry_map(
    entries: &[UsageCatalogEntry],
) -> BTreeMap<UsageAccountCapability, String> {
    entries
        .iter()
        .map(|entry| (entry.capability.clone(), entry.revision.clone()))
        .collect()
}

pub(crate) fn ensure_catalog_matches(
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

pub(crate) fn refresh_binding_outcome(
    binding: &ValidatedCredentialBinding,
    resolver: &dyn ProviderCredentialEnvResolver,
) -> ProviderProbeOutcome {
    match refresh_credential_binding(binding, resolver) {
        ProviderCredentialRefreshOutcome::Snapshot { view, rate_limit } => {
            provider_probe_outcome_with_rate_limit(*view, rate_limit)
        }
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
pub(crate) fn provider_probe_outcome(
    view: jackin_protocol::control::FocusedUsageView,
) -> ProviderProbeOutcome {
    provider_probe_outcome_with_rate_limit(view, None)
}

pub(crate) fn provider_probe_outcome_with_rate_limit(
    view: jackin_protocol::control::FocusedUsageView,
    rate_limit: Option<jackin_usage_provider_core::ProviderRateLimit>,
) -> ProviderProbeOutcome {
    if let Some(rate_limit) = rate_limit {
        return ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::RateLimited,
            message: "usage provider rate limit is active".to_owned(),
            retry_at_epoch: rate_limit.retry_at_epoch,
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
pub(crate) fn honest_probe_message(
    view: &jackin_protocol::control::FocusedUsageView,
    fallback: &str,
) -> String {
    view.last_error
        .as_deref()
        .filter(|message| !message.trim().is_empty())
        .unwrap_or(fallback)
        .to_owned()
}
