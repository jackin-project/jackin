//! Tests for `workspace`.
use super::*;
use jackin_config::test_support::config_with_agents as config_with_agents_for_override;
use jackin_config::{WorkspaceConfig, WorkspaceRoleOverride};

struct TestWorkspace {
    allowed_roles: Vec<String>,
}

impl WorkspaceRoleAccess for TestWorkspace {
    fn allowed_roles(&self) -> &[String] {
        &self.allowed_roles
    }
}

fn ws_with_allowed(allowed: Vec<String>) -> TestWorkspace {
    TestWorkspace {
        allowed_roles: allowed,
    }
}

#[test]
fn allows_all_when_empty() {
    assert!(allows_all_agents(&ws_with_allowed(vec![])));
    assert!(!allows_all_agents(&ws_with_allowed(vec!["alpha".into()])));
}

#[test]
fn effectively_allowed_with_shorthand_or_explicit_membership() {
    let all = ws_with_allowed(vec![]);
    assert!(agent_is_effectively_allowed(&all, "alpha"));
    assert!(agent_is_effectively_allowed(&all, "beta"));

    let custom = ws_with_allowed(vec!["alpha".into(), "gamma".into()]);
    assert!(agent_is_effectively_allowed(&custom, "alpha"));
    assert!(!agent_is_effectively_allowed(&custom, "beta"));
    assert!(agent_is_effectively_allowed(&custom, "gamma"));
}

#[test]
fn eligible_role_keys_for_override_uses_allowed_or_all_roles() {
    let registered = ["alpha".to_owned(), "beta".to_owned()];

    let mut eligible = eligible_role_keys_for_override(registered.iter(), &ws_with_allowed(vec![]));
    eligible.sort();
    assert_eq!(eligible, vec!["alpha".to_owned(), "beta".to_owned()]);

    assert_eq!(
        eligible_role_keys_for_override(registered.iter(), &ws_with_allowed(vec!["ghost".into()])),
        vec!["ghost".to_owned()]
    );
}

fn role(key: &str) -> RoleSelector {
    RoleSelector::parse(key).unwrap()
}

fn role_keys(roles: &[RoleSelector]) -> Vec<String> {
    roles.iter().map(RoleSelector::key).collect()
}

#[test]
fn configured_roles_keeps_only_valid_selectors() {
    let registered = [
        "alpha".to_owned(),
        "invalid role".to_owned(),
        "team/beta".to_owned(),
    ];

    assert_eq!(
        role_keys(&configured_roles(registered.iter())),
        vec!["alpha".to_owned(), "team/beta".to_owned()]
    );
}

#[test]
fn eligible_roles_for_workspace_uses_all_registered_when_allowed_empty() {
    let registered = [
        "alpha".to_owned(),
        "invalid role".to_owned(),
        "team/beta".to_owned(),
    ];

    assert_eq!(
        role_keys(&eligible_roles_for_workspace(
            registered.iter(),
            &ws_with_allowed(vec![])
        )),
        vec!["alpha".to_owned(), "team/beta".to_owned()]
    );
}

#[test]
fn eligible_roles_for_workspace_filters_to_allowed_registered_roles() {
    let registered = ["alpha".to_owned(), "beta".to_owned()];

    assert_eq!(
        role_keys(&eligible_roles_for_workspace(
            registered.iter(),
            &ws_with_allowed(vec!["beta".into(), "ghost".into()])
        )),
        vec!["beta".to_owned()]
    );
}

#[test]
fn preferred_role_index_uses_last_before_default() {
    let eligible = [role("alpha"), role("beta"), role("gamma")];

    assert_eq!(
        preferred_role_index(&eligible, Some("beta"), Some("alpha")),
        Some(1)
    );
}

#[test]
fn preferred_role_index_falls_back_to_default() {
    let eligible = [role("alpha"), role("beta")];

    assert_eq!(
        preferred_role_index(&eligible, Some("ghost"), Some("beta")),
        Some(1)
    );
}

#[test]
fn preferred_role_index_ignores_missing_roles() {
    let eligible = [role("alpha")];

    assert_eq!(
        preferred_role_index(&eligible, Some("ghost"), Some("beta")),
        None
    );
}

fn ws_with_role_overrides(allowed: &[&str], override_agents: &[&str]) -> WorkspaceConfig {
    let mut roles = std::collections::BTreeMap::new();
    for a in override_agents {
        roles.insert((*a).into(), WorkspaceRoleOverride::default());
    }
    WorkspaceConfig {
        allowed_roles: allowed.iter().map(|s| (*s).into()).collect(),
        roles,
        ..WorkspaceConfig::default()
    }
}

#[test]
fn eligible_agents_returns_allowed_when_list_non_empty() {
    let cfg = config_with_agents_for_override(&["agent-smith", "agent-brown", "agent-architect"]);
    let ws = ws_with_role_overrides(&["agent-smith"], &[]);
    let eligible = eligible_role_keys_for_override(cfg.roles.keys(), &ws);
    assert_eq!(eligible, vec!["agent-smith".to_owned()]);
}

#[test]
fn eligible_agents_returns_all_registered_when_allowed_empty() {
    let cfg = config_with_agents_for_override(&["agent-smith", "agent-brown"]);
    let ws = ws_with_role_overrides(&[], &[]);
    let mut eligible = eligible_role_keys_for_override(cfg.roles.keys(), &ws);
    eligible.sort();
    assert_eq!(
        eligible,
        vec!["agent-brown".to_owned(), "agent-smith".to_owned()]
    );
}

#[test]
fn eligible_agents_does_not_filter_by_existing_overrides() {
    // Operators may want to add additional keys to a role that already
    // carries some — the picker must include every allowed role regardless
    // of whether `pending.roles` already lists them.
    let cfg = config_with_agents_for_override(&["agent-smith", "agent-brown"]);
    let ws = ws_with_role_overrides(&["agent-smith", "agent-brown"], &["agent-smith"]);
    let mut eligible = eligible_role_keys_for_override(cfg.roles.keys(), &ws);
    eligible.sort();
    assert_eq!(
        eligible,
        vec!["agent-brown".to_owned(), "agent-smith".to_owned()],
        "agent-smith already has overrides but must still appear so the operator can add another key to it"
    );
}

#[test]
fn eligible_agents_returns_empty_when_no_allowed_and_no_registered() {
    let cfg = config_with_agents_for_override(&[]);
    let ws = ws_with_role_overrides(&[], &[]);
    let eligible = eligible_role_keys_for_override(cfg.roles.keys(), &ws);
    assert!(eligible.is_empty());
}
