// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::RoleSelector;

use crate::workspace::{
    agent_is_effectively_allowed, allows_all_agents, configured_roles,
    eligible_role_keys_for_override, preferred_role_index,
};

struct FixtureWorkspace {
    allowed: Vec<String>,
}

impl crate::workspace::WorkspaceRoleAccess for FixtureWorkspace {
    fn allowed_roles(&self) -> &[String] {
        &self.allowed
    }
}

fn workspace(allowed: &[&str]) -> FixtureWorkspace {
    FixtureWorkspace {
        allowed: allowed.iter().map(|role| (*role).to_owned()).collect(),
    }
}

#[test]
fn empty_allow_list_permits_every_role() {
    let ws = workspace(&[]);
    assert!(allows_all_agents(&ws));
    assert!(agent_is_effectively_allowed(&ws, "anything"));
}

#[test]
fn restricted_allow_list_permits_only_listed_roles() {
    let ws = workspace(&["myrole"]);
    assert!(!allows_all_agents(&ws));
    assert!(agent_is_effectively_allowed(&ws, "myrole"));
    assert!(!agent_is_effectively_allowed(&ws, "other"));
}

#[test]
fn override_keys_fall_back_to_registered_roles_when_unrestricted() {
    let ws = workspace(&[]);
    assert_eq!(
        eligible_role_keys_for_override(["myrole", "other"].into_iter(), &ws),
        ["myrole".to_owned(), "other".to_owned()]
    );
}

#[test]
fn override_keys_use_allow_list_when_restricted() {
    let ws = workspace(&["myrole"]);
    assert_eq!(
        eligible_role_keys_for_override(["myrole", "other"].into_iter(), &ws),
        ["myrole".to_owned()]
    );
}

#[test]
fn configured_roles_skips_unparseable_keys() {
    let roles = configured_roles(["myrole", "not a role!!"].into_iter());
    assert_eq!(
        roles.iter().map(RoleSelector::key).collect::<Vec<_>>(),
        ["myrole".to_owned()]
    );
}

#[test]
fn preferred_role_prefers_last_then_default() {
    let roles = configured_roles(["aaa", "bbb"].into_iter());
    assert_eq!(
        preferred_role_index(&roles, Some("bbb"), Some("aaa")),
        Some(1)
    );
    assert_eq!(preferred_role_index(&roles, None, Some("aaa")), Some(0));
    assert_eq!(
        preferred_role_index(&roles, Some("missing"), Some("aaa")),
        Some(0)
    );
    assert_eq!(preferred_role_index(&roles, None, None), None);
}
