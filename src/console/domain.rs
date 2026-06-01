//! Pure console product rules.

use crate::config::{AppConfig, RoleSource};
use crate::selector::RoleSelector;

/// Resolve the role source the console should load for an operator-entered selector.
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
            env: std::collections::BTreeMap::new(),
        }),
        Err(err) => Err(err),
    }
}
