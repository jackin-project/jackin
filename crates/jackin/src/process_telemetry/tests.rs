// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use super::*;

#[tokio::test]
async fn exports_host_process_matrix_without_operator_material() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    exec_sync(&ExecRequest::new(
        "sh",
        [
            "-c",
            "printf operator-secret-stdout; printf operator-secret-stderr >&2",
        ],
    ))
    .unwrap();
    exec_async(&ExecRequest::new("docker", ["operator-secret-argument"]))
        .await
        .unwrap();
    exec_async(&ExecRequest::new("sh", ["-c", "sleep 1"]).timeout(Duration::from_millis(5)))
        .await
        .unwrap();
    let error = exec_sync(&ExecRequest::new(
        "/operator-secret/missing-command",
        ["operator-secret-spawn-argument"],
    ))
    .unwrap_err();
    assert_eq!(error.to_string(), "process spawn failed");

    assert!(
        exec_sync_optional(&ExecRequest::new(
            "/operator-secret/missing-viewer",
            ["operator-secret-viewer-argument"],
        ))
        .unwrap()
        .is_none()
    );
    {
        let request = ExecRequest::new("sh", ["-c", "exit 0"]);
        let (operation, mut child) = spawn_async(&request).unwrap();
        child.wait().await.unwrap();
        operation.complete_ready();
    }
    {
        let request = ExecRequest::new(
            "/operator-secret/missing-daemon",
            ["operator-secret-daemon-argument"],
        );
        let Err(error) = spawn_async(&request) else {
            panic!("missing daemon unexpectedly spawned");
        };
        assert_eq!(error.to_string(), "process spawn failed");
    }
    {
        let request = ExecRequest::new("sh", ["-c", "exit 0"]);
        let (_operation, mut child) = spawn_async(&request).unwrap();
        child.wait().await.unwrap();
    }

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 8);
    assert_eq!(export.error_span_count(), 6);
    for expected in [
        "sh",
        "docker",
        "other",
        "process_exit_nonzero",
        "process_spawn_error",
        "telemetry_instrumentation_fault",
        "timeout",
    ] {
        assert!(export.contains_span_text(expected));
    }
    for secret in [
        "operator-secret-stdout",
        "operator-secret-stderr",
        "operator-secret-argument",
        "/operator-secret/missing-command",
        "operator-secret-spawn-argument",
        "/operator-secret/missing-viewer",
        "operator-secret-viewer-argument",
        "/operator-secret/missing-daemon",
        "operator-secret-daemon-argument",
    ] {
        assert!(!export.contains_span_text(secret));
    }
}
