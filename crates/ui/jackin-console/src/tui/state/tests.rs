// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn console_error_export_is_bodyless() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        record_console_error(jackin_telemetry::schema::enums::ErrorType::ConfigError);
    });

    export.force_flush();
    assert_eq!(export.event_count("error.typed"), 1);
    assert!(export.contains_log_text("config_error"));
    for private in ["selector", "repository", "popup", "raw error"] {
        assert!(!export.contains_log_text(private));
    }
}
