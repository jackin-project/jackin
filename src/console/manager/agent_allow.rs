//! Allowed-agent semantics for workspace configurations.
//!
//! The on-disk data model uses an "empty `allowed_agents` list = every
//! agent is allowed" shorthand. This module centralizes that rule so the
//! editor, the details pane, and the save-confirmation summary all agree
//! on what a config means.

use crate::workspace::WorkspaceConfig;

/// True when `ws` uses the "all agents allowed" shorthand — i.e. the
/// `allowed_agents` list is empty.
#[must_use]
pub const fn allows_all_agents(ws: &WorkspaceConfig) -> bool {
    ws.allowed_agents.is_empty()
}

/// True when `agent` is effectively allowed to run in `ws`. Covers both
/// the explicit membership case and the "empty = all" shorthand.
#[must_use]
pub fn agent_is_effectively_allowed(ws: &WorkspaceConfig, agent: &str) -> bool {
    ws.allowed_agents.is_empty() || ws.allowed_agents.iter().any(|a| a == agent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceConfig;

    fn ws_with_allowed(allowed: Vec<String>) -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: String::new(),
            mounts: vec![],
            allowed_agents: allowed,
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn allows_all_when_empty() {
        assert!(allows_all_agents(&ws_with_allowed(vec![])));
        assert!(!allows_all_agents(&ws_with_allowed(vec!["alpha".into()])));
    }

    #[test]
    fn effectively_allowed_with_shorthand_or_explicit_membership() {
        // Empty shorthand: every agent is effectively allowed.
        let all = ws_with_allowed(vec![]);
        assert!(agent_is_effectively_allowed(&all, "alpha"));
        assert!(agent_is_effectively_allowed(&all, "beta"));

        // Explicit list: only named agents are effectively allowed.
        let custom = ws_with_allowed(vec!["alpha".into(), "gamma".into()]);
        assert!(agent_is_effectively_allowed(&custom, "alpha"));
        assert!(!agent_is_effectively_allowed(&custom, "beta"));
        assert!(agent_is_effectively_allowed(&custom, "gamma"));
    }
}
