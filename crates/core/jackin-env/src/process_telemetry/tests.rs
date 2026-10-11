// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use super::*;

#[test]
fn op_write_transport_bounds_stdin_execution_and_export() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    let mut success = ExecRequest::new("sh", ["-c", "cat"]);
    success.stdin = Some(b"op-write-secret-body".to_vec());
    success.timeout = Some(Duration::from_secs(1));
    let output = exec_sync_op_with_retry(&success, 1).unwrap();
    assert_eq!(output.stdout, b"op-write-secret-body");

    let mut timeout = ExecRequest::new("sh", ["-c", "sleep 1"]);
    timeout.stdin = Some(b"op-write-secret-timeout-body".to_vec());
    timeout.timeout = Some(Duration::from_millis(5));
    assert!(exec_sync_op_with_retry(&timeout, 1).unwrap().timed_out);

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 2);
    assert_eq!(export.error_span_count(), 1);
    assert!(export.contains_span_text("op"));
    assert!(export.contains_span_text("timeout"));
    for secret in ["op-write-secret-body", "op-write-secret-timeout-body"] {
        assert!(!export.contains_span_text(secret));
    }
}
