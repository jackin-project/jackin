// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Secret-free source facts forwarded from one host launch.

use std::collections::{BTreeMap, BTreeSet};

use jackin_protocol::usage_broker::{UsageCredentialScope, UsageRelayForwardedSourcesV1};

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

impl From<&ForwardedUsageSources> for UsageRelayForwardedSourcesV1 {
    fn from(sources: &ForwardedUsageSources) -> Self {
        Self {
            selected_account_ids: sources.selected_account_ids.clone(),
            selected_account_surfaces: sources.selected_account_surfaces.clone(),
            profile_surface_ids: sources.profile_surface_ids.clone(),
            env_keys: sources.env_keys.clone(),
            credential_scope: sources.credential_scope.clone(),
        }
    }
}

impl From<UsageRelayForwardedSourcesV1> for ForwardedUsageSources {
    fn from(sources: UsageRelayForwardedSourcesV1) -> Self {
        Self {
            selected_account_ids: sources.selected_account_ids,
            selected_account_surfaces: sources.selected_account_surfaces,
            profile_surface_ids: sources.profile_surface_ids,
            env_keys: sources.env_keys,
            credential_scope: sources.credential_scope,
        }
    }
}
