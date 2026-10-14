// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_protocol::usage_broker::UsageAccountCapability;

use crate::{ForwardedUsageSources, capability_for_binding, forwarding_requirement};
use jackin_usage_discovery::{ValidatedCredentialSource, ValidatedUsageDiscovery};

#[must_use]
pub(crate) fn forwarded_usage_capabilities(
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

#[must_use]
pub(crate) fn usage_capability_for_selected_account_with_sources(
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
