// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for role-source helpers.

use std::collections::BTreeMap;

use jackin_config::{AppConfig, RoleSource};
use jackin_core::RoleSelector;

use super::candidate_role_source;

#[test]
fn candidate_role_source_uses_configured_source() {
    let mut config = AppConfig::default();
    config.roles.insert(
        "alpha".into(),
        RoleSource {
            git: "https://example.invalid/alpha.git".into(),
            trusted: true,
            env: BTreeMap::new(),
        },
    );

    let source = candidate_role_source(&config, &RoleSelector::parse("alpha").unwrap()).unwrap();

    assert_eq!(source.git, "https://example.invalid/alpha.git");
    assert!(source.trusted);
}

#[test]
fn candidate_role_source_derives_builtin_bare_role_source() {
    let source = candidate_role_source(
        &AppConfig::default(),
        &RoleSelector::parse("agent-smith").unwrap(),
    )
    .unwrap();

    assert_eq!(
        source.git,
        "https://github.com/jackin-project/jackin-agent-smith.git"
    );
    assert!(!source.trusted);
}

#[test]
fn candidate_role_source_uses_config_rules_for_namespaced_selector() {
    let source = candidate_role_source(
        &AppConfig::default(),
        &RoleSelector::parse("chainargos/architect").unwrap(),
    )
    .unwrap();

    assert_eq!(
        source.git,
        "https://github.com/chainargos/jackin-architect.git"
    );
    assert!(!source.trusted);
}
