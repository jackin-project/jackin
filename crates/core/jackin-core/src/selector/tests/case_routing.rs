// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn routes_container_names_to_container_variant() {
    assert_eq!(
        Selector::parse("jk-worker-1").unwrap(),
        Selector::Container("jk-worker-1".to_owned())
    );
    assert!(matches!(Selector::parse("api").unwrap(), Selector::Role(_)));
}

#[test]
fn runtime_slug_joins_namespace_with_underscore() {
    let role = RoleSelector::parse("team/api").unwrap();
    assert_eq!(runtime_slug(&role), "team_api");
    let plain = RoleSelector::parse("api").unwrap();
    assert_eq!(runtime_slug(&plain), "api");
}
