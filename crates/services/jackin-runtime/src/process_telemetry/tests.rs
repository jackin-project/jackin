// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use super::*;

#[tokio::test]
async fn exports_complete_outcome_matrix_without_process_material() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    let success = ExecRequest::new(
        "sh",
        [
            "-c",
            "printf operator-secret-stdout; printf operator-secret-stderr >&2",
        ],
    );
    exec_async(&success).await.unwrap();

    let nonzero = ExecRequest::new("sh", ["-c", "exit 23"]);
    exec_async(&nonzero).await.unwrap();

    let timeout = ExecRequest::new("sh", ["-c", "sleep 1"]).timeout(Duration::from_millis(5));
    exec_async(&timeout).await.unwrap();

    let missing = ExecRequest::new(
        "operator-secret-missing-program",
        ["operator-secret-argument"],
    );
    let error = exec_async(&missing).await.unwrap_err();
    assert_eq!(error.to_string(), "process spawn failed");

    export.force_flush();
    let spans = export.finished_spans();
    assert_eq!(spans.len(), 4);
    assert!(
        spans
            .iter()
            .all(|span| span.name == jackin_telemetry::schema::spans::PROCESS_COMMAND)
    );
    assert_eq!(export.error_span_count(), 3);
    assert!(export.contains_span_text("process_exit_nonzero"));
    assert!(export.contains_span_text("process_spawn_error"));
    assert!(export.contains_span_text("timeout"));
    assert!(export.contains_span_text("23"));
    for secret in [
        "operator-secret-stdout",
        "operator-secret-stderr",
        "operator-secret-missing-program",
        "operator-secret-argument",
    ] {
        assert!(!export.contains_span_text(secret));
    }
}

#[tokio::test]
async fn child_operations_complete_on_exit_timeout_spawn_and_abandonment() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    let nonzero_request = ExecRequest::new("sh", ["-c", "exit 19"]);
    let (nonzero_operation, mut nonzero_child) = spawn_async(&nonzero_request).unwrap();
    nonzero_operation.complete_status(nonzero_child.wait().await.unwrap());

    let timeout_request = ExecRequest::new("sh", ["-c", "sleep 1"]);
    let (timeout_operation, mut timeout_child) = spawn_async(&timeout_request).unwrap();
    timeout_child.kill().await.unwrap();
    timeout_operation.complete_timeout();

    let missing_request = ExecRequest::new(
        "operator-secret-missing-child",
        ["operator-secret-child-argument"],
    );
    let Err(error) = spawn_async(&missing_request) else {
        panic!("missing executable must fail to spawn");
    };
    assert_eq!(error.to_string(), "process spawn failed");

    let abandoned_request = ExecRequest::new("sh", ["-c", "exit 0"]);
    let (abandoned_operation, mut abandoned_child) = spawn_async(&abandoned_request).unwrap();
    abandoned_child.wait().await.unwrap();
    drop(abandoned_operation);

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 4);
    assert_eq!(export.error_span_count(), 4);
    assert!(export.contains_span_text("19"));
    assert!(export.contains_span_text("process_exit_nonzero"));
    assert!(export.contains_span_text("timeout"));
    assert!(export.contains_span_text("process_spawn_error"));
    assert!(export.contains_span_text("telemetry_instrumentation_fault"));
    assert!(!export.contains_span_text("operator-secret-missing-child"));
    assert!(!export.contains_span_text("operator-secret-child-argument"));
}
