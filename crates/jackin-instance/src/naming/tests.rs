// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `naming`.
use super::*;
use jackin_core::RoleSelector;
use jackin_core::WorkspaceName;

#[test]
fn new_workspace_container_name_is_compact_dns_safe() {
    let selector = RoleSelector::new(Some("chainargos"), "agent-brown");

    let name = container_name_with_id(
        Some(&WorkspaceName::parse("chainargos-project").unwrap()),
        &selector,
        "k7p9m2xq",
    );

    assert_eq!(name, "jk-k7p9m2xq-chainargosproject-agentbrown");
    assert!(is_dns_label(&name));
    assert!(is_dns_label(&format!("{name}-dind")));
}

#[test]
fn new_ad_hoc_container_name_omits_workspace_component() {
    let selector = RoleSelector::new(None, "agent-brown");

    let name = container_name_with_id(None, &selector, "k7p9m2xq");

    assert_eq!(name, "jk-k7p9m2xq-agentbrown");
    assert!(is_dns_label(&name));
}

#[test]
fn long_container_name_fits_dind_dns_budget() {
    let selector = RoleSelector::new(None, "role-name-with-a-very-long-human-friendly-label");

    let name = container_name_with_id(
        Some(
            &WorkspaceName::parse("workspace-name-with-a-very-long-human-friendly-label").unwrap(),
        ),
        &selector,
        "k7p9m2xq",
    );

    assert!(name.len() <= 58, "{name}");
    assert!(is_dns_label(&format!("{name}-dind")));
}

#[test]
fn class_family_matches_new_unique_names_by_visible_role_component() {
    let selector = RoleSelector::new(Some("chainargos"), "agent-brown");

    assert!(class_family_matches(
        &selector,
        "jk-k7p9m2xq-chainargosproject-agentbrown"
    ));
    assert!(!class_family_matches(
        &selector,
        "jk-k7p9m2xq-chainargosproject-agentblue"
    ));
}

#[test]
fn class_family_matches_distinguishes_role_substrings() {
    // A role named `brown` must not match a container whose role
    // component is `agentbrown` (the longer name happens to end
    // in `brown`). Important for `purge_class_data` blast radius.
    let brown = RoleSelector::new(None, "brown");
    assert!(!class_family_matches(&brown, "jk-k7p9m2xq-agentbrown",));
    let agentbrown = RoleSelector::new(None, "agentbrown");
    assert!(!class_family_matches(&agentbrown, "jk-k7p9m2xq-brown",));
    assert!(class_family_matches(&agentbrown, "jk-k7p9m2xq-agentbrown",));
}

#[test]
fn instance_id_from_container_base_extracts_second_component() {
    assert_eq!(
        instance_id_from_container_base("jk-k7p9m2xq-workspace-agentsmith"),
        Some("k7p9m2xq")
    );
    assert_eq!(
        instance_id_from_container_base("jk-k7p9m2xq-agentsmith"),
        Some("k7p9m2xq")
    );
    assert_eq!(instance_id_from_container_base("nojkprefix-k7p9m2xq"), None);
    assert_eq!(instance_id_from_container_base("jk-noid"), None);
}

#[test]
fn class_family_matches_workspace_and_adhoc_for_same_selector() {
    // A single role selector must match both a workspace-scoped container
    // (jk-<id>-<ws>-<role>) and an ad-hoc container (jk-<id>-<role>).
    // The rsplit_once fallback path handles the no-workspace case.
    let selector = RoleSelector::new(None, "agent-smith");
    assert!(class_family_matches(&selector, "jk-k7p9m2xq-agentsmith")); // ad-hoc
    assert!(class_family_matches(
        &selector,
        "jk-k7p9m2xq-myproject-agentsmith" // workspace-scoped
    ));
    assert!(!class_family_matches(&selector, "jk-k7p9m2xq-agentbrown"));
}

#[test]
fn no_workspace_long_role_fits_dns_budget() {
    let selector = RoleSelector::new(None, "role-name-with-a-very-long-human-friendly-label");

    let name = container_name_with_id(None, &selector, "k7p9m2xq");

    assert!(name.len() <= 58, "{name}");
    assert!(is_dns_label(&format!("{name}-dind")));
}

#[test]
fn dind_certs_volume_derives_from_container_name() {
    assert_eq!(
        dind_certs_volume("jk-agent-smith"),
        "jk-agent-smith-dind-certs"
    );
    assert_eq!(
        dind_certs_volume("jk-k7p9m2xq-chainargos-thearchitect"),
        "jk-k7p9m2xq-chainargos-thearchitect-dind-certs"
    );
}
