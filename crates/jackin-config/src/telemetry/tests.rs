// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn nested_config_owners_export_failure_exactly_once_without_error_text() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    let inner: crate::ConfigResult<()> = Err(crate::ConfigError::msg(
        "config-secret-path config-secret-value",
    ));
    let inner = finish_operation(ConfigScope::Workspace, ConfigOperation::Validate, inner);
    let outer = finish_operation(ConfigScope::Global, ConfigOperation::Load, inner);
    assert!(outer.unwrap_err().is_telemetry_owned());

    export.force_flush();
    assert_eq!(export.event_count("config.operation"), 1);
    assert!(export.contains_log_text("config_error"));
    assert!(export.contains_log_text("workspace"));
    assert!(export.contains_log_text("validate"));
    for secret in ["config-secret-path", "config-secret-value"] {
        assert!(!export.contains_log_text(secret));
    }
}
