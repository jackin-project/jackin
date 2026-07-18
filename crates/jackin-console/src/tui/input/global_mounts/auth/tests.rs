// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn missing_settings_auth_return_path_is_bodyless() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, record_missing_auth_return_path);

    export.force_flush();
    assert_eq!(export.event_count("error.typed"), 1);
    assert!(export.contains_log_text("telemetry_instrumentation_fault"));
    for private in ["token", "folder", "op ref", "modal", "path"] {
        assert!(!export.contains_log_text(private));
    }
}
