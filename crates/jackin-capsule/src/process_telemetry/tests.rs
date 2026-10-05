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
    if run_wire_test_in_child(
        "process_telemetry::tests::conformance_wire_exec_spawn_failure_is_owned_once_without_command_material",
        "JACKIN_PROCESS_TELEMETRY_WIRE_CHILD",
    )
    .expect("dispatch isolated process telemetry wire test")
    {
        return;
    }
    let _telemetry_guard = crate::test_support::telemetry_test_guard_async().await;
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

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn cancelled_configured_process_exports_cancellation_without_fault() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    let temporary = tempfile::tempdir().unwrap();
    let ready_path = temporary.path().join("ready");
    let request = ExecRequest::new(
        "sh",
        [
            "-c",
            "printf ready > \"$1\"; exec sleep 30",
            "fixture",
            ready_path.to_str().unwrap(),
        ],
    )
    .no_timeout();
    let mut future = Box::pin(exec_async_as(
        &request,
        ProcessExecutableName::ConfiguredCommand,
    ));
    tokio::select! {
        result = &mut future => panic!("fixture exited before cancellation: {result:?}"),
        ready = tokio::time::timeout(Duration::from_secs(2), async {
            while !ready_path.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }) => ready.expect("real credential/configured process spawned"),
    }
    drop(future);
    drop(guard);
    export.force_flush();
    assert_eq!(export.finished_spans().len(), 1);
    assert_eq!(export.error_span_count(), 0);
    assert!(export.contains_span_text("cancellation"));
    assert!(!export.contains_span_text("telemetry_instrumentation_fault"));
    assert!(!export.contains_span_text("ready"));
}

#[cfg(unix)]
#[test]
fn finite_sync_execution_preserves_accepted_status_and_safe_failure_types() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        let pending = ExecRequest::new("/bin/sh", ["-c", "printf private-response; exit 8"])
            .timeout(Duration::from_secs(2));
        let output = exec_sync_accepted(&pending, &[0, 8]).unwrap();
        assert_eq!(output.code, Some(8));
        assert!(!output.success);

        let missing = ExecRequest::new("/private-marker/missing-command", ["private-argument"])
            .timeout(Duration::from_secs(2));
        let error = exec_sync_accepted(&missing, &[0]).unwrap_err();
        assert_eq!(
            error.downcast_ref::<jackin_process::ExecStage>(),
            Some(&jackin_process::ExecStage::Spawn)
        );
        assert_eq!(format!("{error:#}"), "process spawn failed");
        assert!(!format!("{error:?}").contains("private-marker"));
        assert!(!format!("{error:?}").contains("private-argument"));

        let overflow = ExecRequest::new("/bin/sh", ["-c", "printf private-overflow"])
            .timeout(Duration::from_secs(2))
            .output_limits(1, 1024);
        let error = exec_sync_accepted(&overflow, &[0]).unwrap_err();
        assert!(error.downcast_ref::<std::io::Error>().is_some());
        assert_eq!(format!("{error:#}"), "process I/O failed");
        assert!(!format!("{error:?}").contains("private-overflow"));
    });
    export.force_flush();
    assert_eq!(export.finished_spans().len(), 3);
    assert_eq!(export.error_span_count(), 2);
    assert!(export.contains_span_text("process_spawn_error"));
    assert!(export.contains_span_text("io_error"));
    assert!(!export.contains_span_text("process_exit_nonzero"));
    for private in [
        "private-response",
        "private-marker",
        "private-argument",
        "private-overflow",
    ] {
        assert!(!export.contains_span_text(private));
    }
}
