// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Staged discovery commits and catalog helpers.

use super::{HostRuntimeConfig, HostSurfaceId, HostUsageRuntime, ValidatedUsageDiscovery};
use std::collections::HashSet;

/// A successful discovery scan staged against one runtime catalog generation.
/// The stage is committed only after broker activation succeeds and its base
/// generation still matches.
#[derive(Debug, Clone)]
pub struct StagedUsageDiscovery {
    /// Runtime discovery generation observed before the scan.
    pub base_generation: u64,
    /// Whether the successful scan changes catalog membership or revisions.
    pub changed: bool,
    /// Fresh, validated discovery result.
    pub discovery: ValidatedUsageDiscovery,
}

pub(crate) fn enabled_surface_ids(config: &HostRuntimeConfig) -> Result<HashSet<String>, String> {
    if config.enabled_surface_ids.is_empty() {
        return Ok(HostSurfaceId::ALL
            .iter()
            .map(|surface| surface.id().to_owned())
            .collect());
    }
    let unknown = config
        .enabled_surface_ids
        .iter()
        .filter(|id| HostSurfaceId::from_id(id).is_none())
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(format!(
            "unknown enabled surface ids: {}",
            unknown.join(", ")
        ));
    }
    Ok(config.enabled_surface_ids.iter().cloned().collect())
}

pub(crate) fn discovered_account_keys(
    discovery: Option<&ValidatedUsageDiscovery>,
) -> HashSet<(HostSurfaceId, String)> {
    discovery
        .into_iter()
        .flat_map(|discovery| discovery.accounts.iter())
        .map(|account| (account.identity.surface, account.account_key.clone()))
        .collect()
}

impl Default for HostUsageRuntime {
    fn default() -> Self {
        Self::new()
    }
}
