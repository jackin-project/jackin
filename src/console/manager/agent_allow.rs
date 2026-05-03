//! Allowed-role semantics for workspace configurations.
//!
//! The on-disk data model uses an "empty `allowed_roles` list = every
//! role is allowed" shorthand. This module centralizes that rule so the
//! editor, the details pane, and the save-confirmation summary all agree
//! on what a config means.

use crate::workspace::WorkspaceConfig;

/// True when `ws` uses the "all roles allowed" shorthand — i.e. the
/// `allowed_roles` list is empty.
#[must_use]
pub const fn allows_all_agents(ws: &WorkspaceConfig) -> bool {
    ws.allowed_roles.is_empty()
}

/// True when `role` is effectively allowed to run in `ws`. Covers both
/// the explicit membership case and the "empty = all" shorthand.
#[must_use]
pub fn agent_is_effectively_allowed(ws: &WorkspaceConfig, role: &str) -> bool {
    ws.allowed_roles.is_empty() || ws.allowed_roles.iter().any(|a| a == role)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceConfig;

    fn ws_with_allowed(allowed: Vec<String>) -> WorkspaceConfig {
        WorkspaceConfig {
            allowed_roles: allowed,
            ..WorkspaceConfig::default()
        }
    }

    #[test]
    fn allows_all_when_empty() {
        assert!(allows_all_agents(&ws_with_allowed(vec![])));
        assert!(!allows_all_agents(&ws_with_allowed(vec!["alpha".into()])));
    }

    #[test]
    fn effectively_allowed_with_shorthand_or_explicit_membership() {
        // Empty shorthand: every role is effectively allowed.
        let all = ws_with_allowed(vec![]);
        assert!(agent_is_effectively_allowed(&all, "alpha"));
        assert!(agent_is_effectively_allowed(&all, "beta"));

        // Explicit list: only named roles are effectively allowed.
        let custom = ws_with_allowed(vec!["alpha".into(), "gamma".into()]);
        assert!(agent_is_effectively_allowed(&custom, "alpha"));
        assert!(!agent_is_effectively_allowed(&custom, "beta"));
        assert!(agent_is_effectively_allowed(&custom, "gamma"));
    }
}
