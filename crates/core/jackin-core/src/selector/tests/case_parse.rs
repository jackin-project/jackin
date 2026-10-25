// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parses_plain_role_name() {
    let role = RoleSelector::parse("api").unwrap();
    assert_eq!(role.namespace, None);
    assert_eq!(role.name, "api");
    assert_eq!(role.key(), "api");
}

#[test]
fn lowercases_namespaced_role() {
    let role = RoleSelector::parse("Team/API").unwrap();
    assert_eq!(role.namespace.as_deref(), Some("team"));
    assert_eq!(role.name, "api");
    assert_eq!(role.key(), "team/api");
}

#[test]
fn rejects_empty_selector() {
    assert!(matches!(RoleSelector::parse(""), Err(SelectorError::Empty)));
}

#[test]
fn rejects_overqualified_and_reserved_names() {
    assert!(matches!(
        RoleSelector::parse("a/b/c"),
        Err(SelectorError::Invalid(_))
    ));
    assert!(matches!(
        RoleSelector::parse("jk-worker"),
        Err(SelectorError::Invalid(_))
    ));
}
