// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn pr_context_recovery_export_is_bodyless() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, record_pr_context_recovery);

    export.force_flush();
    assert_eq!(export.event_count("operation.warn"), 1);
    assert!(export.contains_log_text("recovered_degradation"));
    for private in ["pull request", "bucket", "URL", "command", "raw error"] {
        assert!(!export.contains_log_text(private));
    }
}
