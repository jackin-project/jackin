// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use super::*;

#[test]
fn exports_claude_probe_matrix_without_host_material() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    exec_sync_as(
        &ExecRequest::new("sh", ["-c", "printf claude-secret-output"]),
        ProcessExecutableName::Claude,
    )
    .unwrap();
    exec_sync_as(
        &ExecRequest::new("sh", ["-c", "printf claude-secret-stderr >&2; exit 21"]),
        ProcessExecutableName::Claude,
    )
    .unwrap();
    exec_sync_as(
        &ExecRequest::new("sh", ["-c", "sleep 1"]).timeout(Duration::from_millis(5)),
        ProcessExecutableName::Claude,
    )
    .unwrap();
    let error = exec_sync_as(
        &ExecRequest::new("/claude-secret/missing-binary", ["claude-secret-argument"]),
        ProcessExecutableName::Claude,
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "process spawn failed");

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 4);
    assert_eq!(export.error_span_count(), 3);
    for expected in [
        "claude",
        "process_exit_nonzero",
        "process_spawn_error",
        "timeout",
    ] {
        assert!(export.contains_span_text(expected));
    }
    for secret in [
        "claude-secret-output",
        "claude-secret-stderr",
        "/claude-secret/missing-binary",
        "claude-secret-argument",
    ] {
        assert!(!export.contains_span_text(secret));
    }
}

#[test]
fn exports_portable_pty_completion_statuses() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    ChildOperation::begin(ProcessExecutableName::Claude)
        .complete_portable_status(&portable_pty::ExitStatus::with_exit_code(0));
    ChildOperation::begin(ProcessExecutableName::Claude)
        .complete_portable_status(&portable_pty::ExitStatus::with_exit_code(19));

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 2);
    assert_eq!(export.error_span_count(), 1);
    assert!(export.contains_span_text("claude"));
    assert!(export.contains_span_text("process_exit_nonzero"));
    assert!(export.contains_span_text("19"));
}

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
