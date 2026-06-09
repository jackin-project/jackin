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
