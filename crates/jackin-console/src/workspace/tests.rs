//! Tests for `workspace`.
use super::*;

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
