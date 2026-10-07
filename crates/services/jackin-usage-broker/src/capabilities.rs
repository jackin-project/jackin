// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker capability allowlists.

use std::collections::BTreeSet;

use jackin_protocol::usage_broker::UsageAccountCapability;

use crate::{ForwardedUsageSources, capability_for_binding, forwarding_requirement};
use jackin_usage_discovery::{ValidatedCredentialSource, ValidatedUsageDiscovery};
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
                    sources.selected_account_ids.iter().any(|account_id| {
                        provenance == &format!("account {account_id}")
                            && sources
                                .selected_account_surfaces
                                .get(account_id)
                                .is_some_and(|surface| surface == binding.surface.id())
                    })
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
    usage_capability_for_selected_account_with_sources(discovery, account_id, surface_id, None)
}

/// Resolve one exact configured account after intersecting it with the
/// credential sources forwarded into the current Capsule. The source proof is
/// part of launch authority: a provider surface or account id alone cannot
/// select a credential when several routes share that identity.
#[must_use]
pub fn usage_capability_for_selected_account_with_sources(
    discovery: &ValidatedUsageDiscovery,
    account_id: &str,
    surface_id: &str,
    sources: Option<&ForwardedUsageSources>,
) -> Option<UsageAccountCapability> {
    let provenance = format!("account {account_id}");
    let capabilities = discovery
        .bindings
        .iter()
        .filter(|binding| binding.surface.id() == surface_id)
        .filter(|binding| binding.provenance.contains(&provenance))
        .filter(|binding| {
            sources.is_none_or(|sources| match &binding.source {
                // API-key/OAuth routes need exact staged source proof. Profile
                // and forwarded capability behavior stays on the baseline path.
                ValidatedCredentialSource::Env { .. } => {
                    forwarding_requirement(binding).is_forwarded(sources)
                }
                ValidatedCredentialSource::Profile(_)
                | ValidatedCredentialSource::Capability
                | ValidatedCredentialSource::Unpollable => true,
            })
        })
        .map(|binding| capability_for_binding(binding, discovery.config_generation.as_deref()))
        .collect::<BTreeSet<_>>();
    (capabilities.len() == 1)
        .then(|| capabilities.into_iter().next())
        .flatten()
}
