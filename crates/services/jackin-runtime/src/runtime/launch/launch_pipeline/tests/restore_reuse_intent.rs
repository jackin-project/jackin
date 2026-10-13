// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{current_role_reuse_is_compatible, validate_explicit_restore_options};
use crate::runtime::launch::LoadOptions;
use jackin_core::JackinPaths;

#[test]
fn current_role_reuse_rejects_launch_configuration_overrides() {
    assert!(current_role_reuse_is_compatible(&LoadOptions::default()));

    let options = LoadOptions {
        model: Some("gpt-6-luna".to_owned()),
        ..Default::default()
    };
    assert!(!current_role_reuse_is_compatible(&options));

    let options = LoadOptions {
        effort: Some(jackin_core::ReasoningEffort::Max),
        ..Default::default()
    };
    assert!(!current_role_reuse_is_compatible(&options));

    let options = LoadOptions {
        selection: Some(jackin_core::LaunchSelection::Account("work".to_owned())),
        ..Default::default()
    };
    assert!(!current_role_reuse_is_compatible(&options));

    let options = LoadOptions {
        selection: Some(jackin_core::LaunchSelection::Configuration(
            "codex-work".to_owned(),
        )),
        ..Default::default()
    };
    assert!(!current_role_reuse_is_compatible(&options));

    let options = LoadOptions {
        non_interactive: true,
        ..Default::default()
    };
    assert!(!current_role_reuse_is_compatible(&options));
}

#[test]
fn exact_restore_rejects_launch_options_it_cannot_apply() {
    let options = LoadOptions {
        restore_container_base: Some("jk-existing-role".to_owned()),
        model: Some("gpt-6-luna".to_owned()),
        ..LoadOptions::default()
    };
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let error = validate_explicit_restore_options(&paths, &options).unwrap_err();
    assert!(error.to_string().contains("cannot apply"));

    let options = LoadOptions {
        restore_container_base: Some("jk-existing-role".to_owned()),
        effort: Some(jackin_core::ReasoningEffort::Max),
        ..LoadOptions::default()
    };
    assert!(validate_explicit_restore_options(&paths, &options).is_err());

    let options = LoadOptions {
        restore_container_base: Some("jk-existing-role".to_owned()),
        selection: Some(jackin_core::LaunchSelection::Account("work".to_owned())),
        ..LoadOptions::default()
    };
    let error = validate_explicit_restore_options(&paths, &options).unwrap_err();
    assert!(error.to_string().contains("selection"));

    let options = LoadOptions {
        restore_container_base: Some("jk-existing-role".to_owned()),
        non_interactive: true,
        ..LoadOptions::default()
    };
    assert!(validate_explicit_restore_options(&paths, &options).is_err());
}
