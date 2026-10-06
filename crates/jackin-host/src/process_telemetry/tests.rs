// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn child_owner_exports_closed_privacy_safe_outcomes() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    {
        let request = ExecRequest::new("sh", ["-c", "printf clipboard-secret-stderr >&2; exit 17"]);
        let (operation, mut child) = spawn_sync(&request).unwrap();
        operation.complete_status(child.wait().unwrap());
    }
    {
        let request = ExecRequest::new("sh", ["-c", "exit 0"]);
        let (operation, mut child) = spawn_sync(&request).unwrap();
        child.wait().unwrap();
        operation.complete_cancelled();
    }
    {
        let request = ExecRequest::new(
            "/clipboard-secret/missing-command",
            ["clipboard-secret-argument"],
        );
        let Err(error) = spawn_sync(&request) else {
            panic!("missing clipboard executable unexpectedly spawned");
        };
        assert_eq!(error.to_string(), "process spawn failed");
    }
    {
        let request = ExecRequest::new("sh", ["-c", "exit 0"]);
        let (_operation, mut child) = spawn_sync(&request).unwrap();
        child.wait().unwrap();
    }

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 4);
    assert_eq!(export.error_span_count(), 3);
    for expected in [
        "sh",
        "other",
        "process_exit_nonzero",
        "process_spawn_error",
        "telemetry_instrumentation_fault",
        "cancellation",
    ] {
        assert!(export.contains_span_text(expected));
    }
    for secret in [
        "clipboard-secret-stderr",
        "/clipboard-secret/missing-command",
        "clipboard-secret-argument",
    ] {
        assert!(!export.contains_span_text(secret));
    }
}
