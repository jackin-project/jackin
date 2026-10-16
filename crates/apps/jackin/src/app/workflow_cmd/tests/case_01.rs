// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::apply_model_override;

#[test]
fn plan_model_override_wins_over_the_resolved_default() {
    assert_eq!(
        apply_model_override(Some("vendor-default".to_owned()), Some("haiku")),
        Some("haiku".to_owned())
    );
}

#[test]
fn plan_model_override_fills_an_absent_default() {
    assert_eq!(
        apply_model_override(None, Some("haiku")),
        Some("haiku".to_owned())
    );
}

#[test]
fn absent_plan_model_override_keeps_the_resolved_default() {
    assert_eq!(
        apply_model_override(Some("role-model".to_owned()), None),
        Some("role-model".to_owned())
    );
}

#[test]
fn blank_plan_model_override_keeps_the_resolved_default() {
    assert_eq!(
        apply_model_override(Some("role-model".to_owned()), Some("   ")),
        Some("role-model".to_owned())
    );
}

#[test]
fn plan_model_override_trims_surrounding_whitespace() {
    assert_eq!(
        apply_model_override(None, Some("  haiku  ")),
        Some("haiku".to_owned())
    );
}

#[test]
fn frozen_check_argv_rejects_arguments_the_executor_would_drop() {
    let argv = vec![
        "python3".to_owned(),
        "/jackin/checks/check_add.py".to_owned(),
        "/work/repo".to_owned(),
    ];
    super::super::validate_check_argv(&argv, "repair.pkl").expect("supported frozen argv");
    let mut extra = argv.clone();
    extra.push("--required-check".to_owned());
    assert!(super::super::validate_check_argv(&extra, "repair.pkl").is_err());
    let mut traversal = argv;
    traversal[1] = "/jackin/checks/../foreign.py".to_owned();
    assert!(super::super::validate_check_argv(&traversal, "repair.pkl").is_err());
}
