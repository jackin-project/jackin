//! Role-source resolution helpers for console role loading.

use std::collections::BTreeMap;

use jackin_config::{AppConfig, RoleSource};
use jackin_core::RoleSelector;

/// Resolve a configured role source, or derive the default GitHub source for a
/// bare built-in role name.
///
/// Namespaced selectors are delegated to `AppConfig::resolve_role_source` so
/// config-owned selector rules stay authoritative.
pub fn candidate_role_source(
    config: &AppConfig,
    selector: &RoleSelector,
) -> anyhow::Result<RoleSource> {
    let mut candidate = config.clone();
    match candidate.resolve_role_source(selector) {
        Ok((source, _)) => Ok(source),
        Err(_) if selector.namespace.is_none() => Ok(RoleSource {
            git: format!(
                "https://github.com/jackin-project/jackin-{}.git",
                selector.name
            ),
            trusted: false,
            env: BTreeMap::new(),
        }),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests;
