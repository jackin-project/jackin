// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
        "process_telemetry::tests::case_01::conformance_wire_exec_spawn_failure_is_owned_once_without_command_material",
        "JACKIN_PROCESS_TELEMETRY_WIRE_CHILD",
    )
    .expect("dispatch isolated process telemetry wire test")
    {
        return;
    }
    let _telemetry_guard = crate::support::telemetry_test_guard_async().await;
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
        let status = child.wait().unwrap();
        drop(child);
        operation.complete_status(status, &[0]);

        let timeout = ExecRequest::new("sh", ["-c", "sleep 1"]);
        let (operation, mut child) = spawn_sync(&timeout).unwrap();
        child.kill().unwrap();
        let waited = child.wait();
        drop(child);
        drop(waited);
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

#[cfg(target_os = "macos")]
#[test]
fn consuming_wait_with_output_holds_registration_until_child_reaped() {
    use std::sync::mpsc;
    use std::thread;

    struct WaitCompletion {
        release_path: std::path::PathBuf,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl WaitCompletion {
        fn release_and_join(&mut self) -> (std::io::Result<()>, thread::Result<()>) {
            let release = std::fs::write(&self.release_path, b"release");
            let joined = self.worker.take().map_or(Ok(()), thread::JoinHandle::join);
            (release, joined)
        }
    }

    impl Drop for WaitCompletion {
        fn drop(&mut self) {
            if let Some(worker) = self.worker.take() {
                drop(std::fs::write(&self.release_path, b"release"));
                drop(worker.join());
            }
        }
    }

    let directory = tempfile::tempdir().unwrap();
    let ready_path = directory.path().join("ready");
    let entered_path = directory.path().join("entered-wait");
    let release_path = directory.path().join("release");
    let script = concat!(
        "printf ready > \"$1\"; ",
        "read_status=0; while [ \"$read_status\" -eq 0 ]; do ",
        "IFS= read -r -t 1 line; read_status=$?; ",
        "[ \"$read_status\" -eq 0 ] && exit 98; done; ",
        "[ \"$read_status\" -eq 1 ] || exit 98; ",
        "printf entered > \"$2\"; ",
        "n=0; while [ ! -f \"$3\" ] && [ \"$n\" -lt 100 ]; do ",
        "n=$((n + 1)); sleep 0.01; done; ",
        "[ -f \"$3\" ] || exit 99; printf captured; exit 0"
    );
    let request = ExecRequest::new(
        "/bin/bash",
        [
            "-c",
            script,
            "fixture",
            ready_path.to_str().unwrap(),
            entered_path.to_str().unwrap(),
            release_path.to_str().unwrap(),
        ],
    )
    .stdin_mode(jackin_process::StdioMode::Capture)
    .stdout_mode(jackin_process::StdioMode::Capture)
    .stderr_mode(jackin_process::StdioMode::Null);
    let (operation, child) = spawn_sync(&request).unwrap();
    let pid = child.id();
    let (output_sender, output_receiver) = mpsc::sync_channel(1);
    let wait_thread = thread::spawn(move || {
        drop(output_sender.send(child.wait_with_output()));
    });
    let mut cleanup = WaitCompletion {
        release_path: release_path.clone(),
        worker: Some(wait_thread),
    };
    let wait_for_marker = |path: &std::path::Path, timeout: Duration| {
        let deadline = Instant::now() + timeout;
        while !path.is_file() && Instant::now() < deadline {
            thread::yield_now();
        }
        path.is_file()
    };
    let ready_observed = wait_for_marker(&ready_path, Duration::from_secs(1));
    let entered_wait_observed = wait_for_marker(&entered_path, Duration::from_secs(1));
    let retained_during_wait =
        jackin_process::coordinate_children(|registry| registry.contains(pid));
    let (output_still_waiting, early_output) = match output_receiver.try_recv() {
        Ok(output) => (false, Some(Ok(output))),
        Err(mpsc::TryRecvError::Empty) => (true, None),
        Err(mpsc::TryRecvError::Disconnected) => {
            (false, Some(Err(mpsc::RecvTimeoutError::Disconnected)))
        }
    };
    let (release_result, worker_result) = cleanup.release_and_join();
    let output_result =
        early_output.unwrap_or_else(|| output_receiver.recv_timeout(Duration::from_secs(1)));
    let worker_joined = worker_result.is_ok();
    let (output_success, output_matches) = if let Ok(Ok(output)) = output_result {
        let success = output.status.success();
        let matches = output.stdout == b"captured";
        operation.complete_status(output.status, &[0]);
        (success, matches)
    } else {
        operation.complete_io_failure();
        (false, false)
    };
    let released_after_wait =
        !jackin_process::coordinate_children(|registry| registry.contains(pid));

    assert!(ready_observed, "child reached the bounded stdin rendezvous");
    assert!(
        entered_wait_observed,
        "wait_with_output closed stdin before the child rendezvous"
    );
    assert!(
        retained_during_wait,
        "wait_with_output keeps the owner while blocked"
    );
    assert!(
        output_still_waiting,
        "the release gate holds child completion"
    );
    assert!(
        release_result.is_ok(),
        "release the exact consuming-wait fixture"
    );
    assert!(worker_joined, "the bounded consuming-wait worker joined");
    assert!(
        output_success,
        "consuming wait reports successful child exit"
    );
    assert!(output_matches, "consuming wait preserves captured stdout");
    assert!(
        released_after_wait,
        "consuming wait releases its PID only after the child has been reaped"
    );
}
