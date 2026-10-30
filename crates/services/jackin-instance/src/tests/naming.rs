// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::naming::{
    class_family_matches, compact_component, container_name_with_id, is_dns_label,
};
use jackin_core::{RoleSelector, WorkspaceName};

fn selector() -> RoleSelector {
    RoleSelector {
        namespace: None,
        name: "payments-api".to_owned(),
    }
}

#[test]
fn compact_component_normalizes_text_and_falls_back() {
    assert_eq!(compact_component("Hello_World-9", "fb"), "helloworld9");
    assert_eq!(compact_component("!!!", "fallback"), "fallback");
}

#[test]
fn generated_name_is_stable_dns_label() {
    let workspace = WorkspaceName::parse("demo").expect("valid workspace name");
    let first = container_name_with_id(Some(&workspace), &selector(), "abc123xy");
    let second = container_name_with_id(Some(&workspace), &selector(), "abc123xy");
    assert_eq!(first, second);
    assert!(is_dns_label(&first));
    assert!(first.starts_with("jk-"));
    assert!(first.contains("demo"));
    assert!(first.contains("paymentsapi"));
}

#[test]
fn class_family_matches_own_name_only() {
    let workspace = WorkspaceName::parse("demo").expect("valid workspace name");
    let name = container_name_with_id(Some(&workspace), &selector(), "abc123xy");
    assert!(class_family_matches(&selector(), &name));
    assert!(!class_family_matches(&selector(), "jk-deadbeef-other"));
}
