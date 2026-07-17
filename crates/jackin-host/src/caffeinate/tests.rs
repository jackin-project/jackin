// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `caffeinate`.
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use super::*;
use jackin_core::ContainerRow;
use jackin_test_support::{FakeDockerClient, FakeRunner};
use tempfile::tempdir;

#[tokio::test]
async fn count_keep_awake_agents_returns_zero_for_empty_output() {
    let docker = FakeDockerClient::default();
    let count = count_keep_awake_agents(&docker).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn count_keep_awake_agents_counts_nonempty_lines() {
    let docker = FakeDockerClient {
        list_containers_queue: RefCell::new(VecDeque::from([vec![
            ContainerRow {
                name: "jk-agent-smith".to_owned(),
                labels: HashMap::default(),
            },
            ContainerRow {
                name: "jk-the-architect".to_owned(),
                labels: HashMap::default(),
            },
        ]])),
        ..Default::default()
    };
    let count = count_keep_awake_agents(&docker).await.unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn reconcile_gate_false_with_no_pidfile_skips_docker_count() {
    let tmp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    reconcile_inner(&paths, &docker, &mut runner, false)
        .await
        .unwrap();

    // Canonical fake records list_containers as `docker ps…` entries.
    let list_calls = docker
        .recorded
        .borrow()
        .iter()
        .filter(|op| op.contains("docker ps"))
        .count();
    assert_eq!(list_calls, 0);
}

#[test]
fn read_pid_file_returns_none_when_missing() {
    let tmp = tempdir().unwrap();
    assert_eq!(
        read_pid_file(&tmp.path().join("missing.pid")).unwrap(),
        None
    );
}

#[test]
fn read_pid_file_parses_trimmed_pid() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("p.pid");
    std::fs::write(&path, "12345\n").unwrap();
    assert_eq!(read_pid_file(&path).unwrap(), Some(12345));
}

#[test]
fn read_pid_file_returns_none_for_empty_file() {
    // Empty file is the legitimate "no PID recorded" state — treat
    // as a fresh start, not as corruption.
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("p.pid");
    std::fs::write(&path, "").unwrap();
    assert_eq!(read_pid_file(&path).unwrap(), None);
}

#[test]
fn read_pid_file_errors_on_garbage() {
    // Corrupted PID file (non-numeric content) MUST surface as an
    // error rather than coercing to None — silent coercion would
    // let the next `(true, Gone)` arm spawn a duplicate caffeinate
    // over the unrecorded survivor, orphaning the prior process
    // until reboot.
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("p.pid");
    std::fs::write(&path, "not-a-pid").unwrap();
    let err = read_pid_file(&path).unwrap_err();
    assert!(
        err.to_string().contains("non-numeric"),
        "error must mention non-numeric content; got: {err}",
    );
}

#[test]
fn is_caffeinate_alive_at_returns_gone_for_nonexistent_pid() {
    // PID 1 always exists; pick a deliberately huge number unlikely
    // to be allocated. `ps -p` returns nonzero for missing PIDs.
    assert_eq!(is_caffeinate_alive_at(2_000_000_000), Liveness::Gone);
}

#[test]
fn liveness_probe_exports_nonzero_without_pid_or_output() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        assert_eq!(is_caffeinate_alive_at(2_000_000_000), Liveness::Gone);
    });
    export.force_flush();

    assert_eq!(export.finished_spans().len(), 1);
    assert_eq!(export.error_span_count(), 1);
    assert!(export.contains_span_text("process_exit_nonzero"));
    assert!(export.contains_span_text("ps"));
    assert!(!export.contains_span_text("2000000000"));
}

#[test]
fn is_caffeinate_alive_at_returns_gone_for_unrelated_process() {
    // PID 1 is launchd on macOS / init on Linux — alive, but its
    // comm is not "caffeinate". This is exactly the PID-reuse race
    // the comm check guards against; the impostor must classify as
    // `Gone`, not `Alive`, so the caller never SIGTERMs it.
    assert_eq!(is_caffeinate_alive_at(1), Liveness::Gone);
}

#[test]
fn classify_ps_comm_output_returns_gone_on_nonzero_exit() {
    // `ps -p <missing>` exits nonzero with empty stdout — that's
    // the "no such process" signal.
    assert_eq!(classify_ps_comm_output(false, ""), Liveness::Gone);
}

#[test]
fn classify_ps_comm_output_returns_alive_for_basename() {
    // Linux-style: `ps -o comm=` reports just the basename.
    assert_eq!(
        classify_ps_comm_output(true, "caffeinate\n"),
        Liveness::Alive
    );
}

#[test]
fn classify_ps_comm_output_returns_alive_for_absolute_path() {
    // macOS-style: `ps -o comm=` reports the full executable path.
    assert_eq!(
        classify_ps_comm_output(true, "/usr/bin/caffeinate\n"),
        Liveness::Alive,
    );
}

#[test]
fn classify_ps_comm_output_returns_gone_for_other_process() {
    // PID alive but comm doesn't match — same outcome as "no such
    // PID": treat as gone, never act on it.
    assert_eq!(
        classify_ps_comm_output(true, "/sbin/launchd\n"),
        Liveness::Gone
    );
    assert_eq!(classify_ps_comm_output(true, "bash\n"), Liveness::Gone);
}

#[test]
fn classify_ps_comm_output_does_not_match_substring() {
    // Guard against a future "simplification" to `contains` that
    // would treat `caffeinated`, `xcaffeinate`, etc. as a match.
    assert_eq!(
        classify_ps_comm_output(true, "caffeinated\n"),
        Liveness::Gone
    );
    assert_eq!(
        classify_ps_comm_output(true, "xcaffeinate\n"),
        Liveness::Gone
    );
}

#[test]
fn classify_ps_comm_output_returns_gone_for_empty_stdout() {
    // Defensive: success + empty output shouldn't be treated as a
    // match (basename == "" != "caffeinate").
    assert_eq!(classify_ps_comm_output(true, ""), Liveness::Gone);
}

#[tokio::test]
async fn reconcile_inner_is_noop_when_no_agents_and_no_pid_file() {
    let tmp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    reconcile_inner(&paths, &docker, &mut runner, true)
        .await
        .unwrap();

    assert!(!pid_path_for_tests(&paths).exists());
}

#[tokio::test]
async fn reconcile_inner_clears_stale_pid_file_when_no_agents() {
    let tmp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let pid_path = pid_path_for_tests(&paths);
    std::fs::write(&pid_path, "2000000001").unwrap();

    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();
    reconcile_inner(&paths, &docker, &mut runner, true)
        .await
        .unwrap();

    assert!(!pid_path.exists(), "stale PID file should be removed");
}

#[tokio::test]
async fn reconcile_inner_clears_pid_file_when_pid_belongs_to_unrelated_process() {
    let tmp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let pid_path = pid_path_for_tests(&paths);
    std::fs::write(&pid_path, "1").unwrap();

    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();
    reconcile_inner(&paths, &docker, &mut runner, true)
        .await
        .unwrap();

    assert!(
        !pid_path.exists(),
        "PID file pointing at an unrelated live process should be removed"
    );
}
