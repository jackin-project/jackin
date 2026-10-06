// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Attached broker handle and forwarded sources.

use std::collections::{BTreeMap, BTreeSet};

use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCredentialScope};

use super::{ScopedCapability, UsageBrokerClient};

/// Attached broker plus every host-discovered canonical capability.
#[derive(Debug, Clone)]
pub struct UsageBrokerHandle {
    /// Host-only transport client.
    pub client: UsageBrokerClient,
    /// Canonical accounts known to this discovery generation.
    pub capabilities: Vec<UsageAccountCapability>,
    /// Publication lease fencing this activation's capability set.
    pub catalog_lease: String,
    pub(crate) scoped_capabilities: BTreeMap<String, Vec<ScopedCapability>>,
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
