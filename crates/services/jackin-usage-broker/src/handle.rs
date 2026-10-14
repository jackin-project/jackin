// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use jackin_protocol::usage_broker::{UsageCredentialScope, UsageRelayForwardedSourcesV1};

#[derive(Debug, Clone, Default)]
pub struct ForwardedUsageSources {
    pub selected_account_ids: BTreeSet<String>,
    pub selected_account_surfaces: BTreeMap<String, String>,
    pub profile_surface_ids: BTreeSet<String>,
    pub env_keys: BTreeSet<String>,
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
