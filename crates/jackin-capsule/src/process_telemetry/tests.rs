// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, Instant};

use super::*;

#[tokio::test]
async fn exports_capsule_process_matrix_without_operator_material() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    let success = ExecRequest::new(
        "sh",
        [
            "-c",
            "printf operator-secret-stdout; printf operator-secret-stderr >&2",
        ],
    );
    exec_async_as(&success, ProcessExecutableName::ConfiguredCommand)
        .await
        .unwrap();

    let nonzero = ExecRequest::new("git", ["operator-secret-argument"]);
    exec_async_as(&nonzero, ProcessExecutableName::Git)
        .await
        .unwrap();

    let timeout = ExecRequest::new("sh", ["-c", "sleep 1"]).timeout(Duration::from_millis(5));
    exec_async_as(&timeout, ProcessExecutableName::ConfiguredCommand)
        .await
        .unwrap();

    let missing = ExecRequest::new(
        "/operator-secret/missing-command",
        ["operator-secret-spawn-argument"],
    );
    let error = exec_async_as(&missing, ProcessExecutableName::ConfiguredCommand)
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "process spawn failed");

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 4);
    assert_eq!(export.error_span_count(), 3);
    assert!(export.contains_span_text("configured_command"));
    assert!(export.contains_span_text("git"));
    assert!(export.contains_span_text("process_exit_nonzero"));
    assert!(export.contains_span_text("process_spawn_error"));
    assert!(export.contains_span_text("timeout"));
    for secret in [
        "operator-secret-stdout",
        "operator-secret-stderr",
        "operator-secret-argument",
        "/operator-secret/missing-command",
        "operator-secret-spawn-argument",
    ] {
        assert!(!export.contains_span_text(secret));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conformance_wire_exec_spawn_failure_is_owned_once_without_command_material() {
    let testbed = jackin_otlp_testbed::Testbed::start().expect("start OTLP testbed");
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::CAPSULE,
    )
    .expect("initialize wire test export");

    let request = ExecRequest::new(
        "/wire-secret/missing-command",
        ["wire-secret-argument", "wire-secret-token"],
    );
    let error = exec_async_as(&request, ProcessExecutableName::ConfiguredCommand)
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "process spawn failed");
    jackin_diagnostics::flush_wire_test_export().expect("flush wire test export");

    let deadline = Instant::now() + Duration::from_secs(2);
    let spans = loop {
        let spans = testbed
            .spans()
            .into_iter()
            .filter(|span| span.name == "process.command")
            .collect::<Vec<_>>();
        if spans.len() == 1 {
            break spans;
        }
        assert!(
            Instant::now() < deadline,
            "process command wire span did not arrive exactly once"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    let wire_text = format!("{spans:?}");
    for expected in ["configured_command", "failure", "process_spawn_error"] {
        assert!(
            wire_text.contains(expected),
            "missing {expected}: {wire_text}"
        );
    }
    let prohibited = [
        "/wire-secret/missing-command",
        "wire-secret-argument",
        "wire-secret-token",
    ];
    for value in prohibited {
        assert!(!wire_text.contains(value), "exported {value}");
    }
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
}

#[test]
fn child_owner_exports_exit_timeout_spawn_and_abandonment() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        let nonzero = ExecRequest::new("sh", ["-c", "exit 19"]);
        let (operation, mut child) = spawn_sync(&nonzero).unwrap();
        operation.complete_status(child.wait().unwrap(), &[0]);

        let timeout = ExecRequest::new("sh", ["-c", "sleep 1"]);
        let (operation, mut child) = spawn_sync(&timeout).unwrap();
        child.kill().unwrap();
        drop(child.wait());
        operation.complete_timeout();

        let missing = ExecRequest::new(
            "/operator-secret/missing-child",
            ["operator-secret-child-argument"],
        );
        let Err(error) = spawn_sync(&missing) else {
            panic!("missing executable must fail to spawn");
        };
        assert_eq!(error.to_string(), "process spawn failed");

        let abandoned = ExecRequest::new("sh", ["-c", "exit 0"]);
        let (operation, mut child) = spawn_sync(&abandoned).unwrap();
        drop(child.wait());
        drop(operation);
    });
    export.force_flush();

    assert_eq!(export.finished_spans().len(), 4);
    assert_eq!(export.error_span_count(), 4);
    assert!(export.contains_span_text("process_exit_nonzero"));
    assert!(export.contains_span_text("process_spawn_error"));
    assert!(export.contains_span_text("timeout"));
    assert!(export.contains_span_text("telemetry_instrumentation_fault"));
    assert!(!export.contains_span_text("/operator-secret/missing-child"));
    assert!(!export.contains_span_text("operator-secret-child-argument"));
}
