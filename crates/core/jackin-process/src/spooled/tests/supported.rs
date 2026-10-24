// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(all(
    unix,
    any(
        target_os = "macos",
        all(target_os = "linux", not(target_env = "uclibc"))
    )
))]

use super::super::*;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt as _;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;
use std::time::Instant;

use super::support::*;

const INHERITED_INPUT_WORKER_ENV: &str = "JACKIN_SPOOLED_INHERITED_INPUT_WORKER";

fn run_inherited_input_worker() {
    use std::process::{Command, Stdio};

    let executable = std::env::current_exe().expect("test executable path");
    let worker_name = "spooled::tests::supported::worker_inherited_piped_stdin_line";
    let mut child = Command::new(executable)
        .args(["--exact", worker_name, "--nocapture"])
        .env(INHERITED_INPUT_WORKER_ENV, worker_name)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn isolated inherited-input test worker");
    child
        .stdin
        .take()
        .expect("worker stdin is piped")
        .write_all(b"inherited-line\n")
        .expect("write one known line to the worker's stdin");

    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match child.try_wait().expect("poll inherited-input worker") {
            Some(status) => {
                assert!(status.success(), "inherited-input worker failed: {status}");
                break;
            }
            None if Instant::now() < deadline => {
                owned_group::process_supervision_poll(Some(deadline));
            }
            None => {
                stop_known_child(&mut child);
                panic!("inherited-input worker exceeded its test deadline");
            }
        }
    }
}

fn stop_known_child(child: &mut std::process::Child) {
    if child
        .try_wait()
        .expect("poll known test worker handle")
        .is_none()
    {
        child.kill().expect("kill known test worker handle");
        child.wait().expect("reap known test worker handle");
    }
}

#[test]
fn output_pair_uses_retained_named_private_single_link_files() {
    let spool_files = output_pair();
    for path in [spool_files.stdout_path(), spool_files.stderr_path()] {
        let metadata = std::fs::metadata(path).expect("inspect retained named spool fixture");
        assert_eq!(metadata.mode() & 0o7777, 0o600);
        assert_eq!(metadata.nlink(), 1);
        assert!(metadata.is_file());
    }
    let (stdout, stderr) = spool_files.files();
    for file in [&stdout, &stderr] {
        let flags = nix::fcntl::fcntl(file, nix::fcntl::FcntlArg::F_GETFD)
            .expect("read retained spool descriptor flags");
        assert!(
            nix::fcntl::FdFlag::from_bits_truncate(flags).contains(nix::fcntl::FdFlag::FD_CLOEXEC)
        );
    }
}

#[test]
fn inheritable_stdout_spool_is_rejected_before_spawn() {
    let spool_files = output_pair();
    let marker = spool_files.stdout_path().with_file_name("target-started");
    assert!(!marker.exists());
    let stdout = File::from(
        nix::fcntl::open(
            &spool_files.stdout_path(),
            nix::fcntl::OFlag::O_WRONLY | nix::fcntl::OFlag::O_NOFOLLOW,
            nix::sys::stat::Mode::empty(),
        )
        .expect("open stdout spool without close-on-exec"),
    );
    let stderr = spool_files.stderr_file();
    let stdout_fd = stdout.as_raw_fd();
    let stderr_fd = stderr.as_raw_fd();
    let stdout_identity = stdout.metadata().expect("inspect inheritable stdout");
    let stderr_identity = stderr.metadata().expect("inspect valid stderr");
    assert!(
        !nix::fcntl::FdFlag::from_bits_truncate(
            nix::fcntl::fcntl(&stdout, nix::fcntl::FcntlArg::F_GETFD)
                .expect("read inheritable stdout flags")
        )
        .contains(nix::fcntl::FdFlag::FD_CLOEXEC)
    );
    assert!(
        nix::fcntl::FdFlag::from_bits_truncate(
            nix::fcntl::fcntl(&stderr, nix::fcntl::FcntlArg::F_GETFD).expect("read stderr flags")
        )
        .contains(nix::fcntl::FdFlag::FD_CLOEXEC)
    );

    let request = ExecRequest::new("/bin/sh", ["-c", ": > \"$JACKIN_SPOOL_MARKER\""])
        .envs([("JACKIN_SPOOL_MARKER", marker.as_os_str())])
        .timeout(Duration::from_secs(2))
        .output_limits(8, 8);
    let error = typed_error(
        exec_spooled_sync_cancellable(&request, stdout, stderr, &AtomicBool::new(false))
            .expect_err("inheritable stdout must be rejected before spawn"),
    );
    assert_eq!(error.kind, SpoolFailureKind::Request);
    assert_eq!(
        error.message,
        "stdout spool descriptor must have FD_CLOEXEC"
    );
    assert_eq!(error.stdout_bytes, 0);
    assert_eq!(error.stderr_bytes, 0);
    let (stdout, stderr, recovery) = error.into_parts();
    assert!(recovery.is_none());
    let stdout = stdout.expect("return original stdout descriptor");
    let stderr = stderr.expect("return original stderr descriptor");
    assert_eq!(stdout.as_raw_fd(), stdout_fd);
    assert_eq!(stderr.as_raw_fd(), stderr_fd);
    let returned_stdout = stdout.metadata().expect("inspect returned stdout");
    let returned_stderr = stderr.metadata().expect("inspect returned stderr");
    assert_eq!(
        (returned_stdout.dev(), returned_stdout.ino()),
        (stdout_identity.dev(), stdout_identity.ino())
    );
    assert_eq!(
        (returned_stderr.dev(), returned_stderr.ino()),
        (stderr_identity.dev(), stderr_identity.ino())
    );
    assert_eq!(returned_stdout.len(), 0);
    assert_eq!(returned_stderr.len(), 0);
    assert!(
        !nix::fcntl::FdFlag::from_bits_truncate(
            nix::fcntl::fcntl(&stdout, nix::fcntl::FcntlArg::F_GETFD)
                .expect("re-read returned stdout flags")
        )
        .contains(nix::fcntl::FdFlag::FD_CLOEXEC)
    );
    assert!(
        nix::fcntl::FdFlag::from_bits_truncate(
            nix::fcntl::fcntl(&stderr, nix::fcntl::FcntlArg::F_GETFD)
                .expect("re-read returned stderr flags")
        )
        .contains(nix::fcntl::FdFlag::FD_CLOEXEC)
    );
    assert!(
        !marker.exists(),
        "rejected request must not start its target"
    );
}

#[test]
fn inheritable_stderr_spool_is_rejected_before_spawn() {
    let spool_files = output_pair();
    let marker = spool_files.stdout_path().with_file_name("target-started");
    assert!(!marker.exists());
    let stdout = spool_files.stdout_file();
    let stderr = File::from(
        nix::fcntl::open(
            &spool_files.stderr_path(),
            nix::fcntl::OFlag::O_WRONLY | nix::fcntl::OFlag::O_NOFOLLOW,
            nix::sys::stat::Mode::empty(),
        )
        .expect("open stderr spool without close-on-exec"),
    );
    let stdout_fd = stdout.as_raw_fd();
    let stderr_fd = stderr.as_raw_fd();
    let stdout_identity = stdout.metadata().expect("inspect valid stdout");
    let stderr_identity = stderr.metadata().expect("inspect inheritable stderr");
    assert!(
        nix::fcntl::FdFlag::from_bits_truncate(
            nix::fcntl::fcntl(&stdout, nix::fcntl::FcntlArg::F_GETFD).expect("read stdout flags")
        )
        .contains(nix::fcntl::FdFlag::FD_CLOEXEC)
    );
    assert!(
        !nix::fcntl::FdFlag::from_bits_truncate(
            nix::fcntl::fcntl(&stderr, nix::fcntl::FcntlArg::F_GETFD)
                .expect("read inheritable stderr flags")
        )
        .contains(nix::fcntl::FdFlag::FD_CLOEXEC)
    );

    let request = ExecRequest::new("/bin/sh", ["-c", ": > \"$JACKIN_SPOOL_MARKER\""])
        .envs([("JACKIN_SPOOL_MARKER", marker.as_os_str())])
        .timeout(Duration::from_secs(2))
        .output_limits(8, 8);
    let error = typed_error(
        exec_spooled_sync_cancellable(&request, stdout, stderr, &AtomicBool::new(false))
            .expect_err("inheritable stderr must be rejected before spawn"),
    );
    assert_eq!(error.kind, SpoolFailureKind::Request);
    assert_eq!(
        error.message,
        "stderr spool descriptor must have FD_CLOEXEC"
    );
    assert_eq!(error.stdout_bytes, 0);
    assert_eq!(error.stderr_bytes, 0);
    let (stdout, stderr, recovery) = error.into_parts();
    assert!(recovery.is_none());
    let stdout = stdout.expect("return original stdout descriptor");
    let stderr = stderr.expect("return original stderr descriptor");
    assert_eq!(stdout.as_raw_fd(), stdout_fd);
    assert_eq!(stderr.as_raw_fd(), stderr_fd);
    let returned_stdout = stdout.metadata().expect("inspect returned stdout");
    let returned_stderr = stderr.metadata().expect("inspect returned stderr");
    assert_eq!(
        (returned_stdout.dev(), returned_stdout.ino()),
        (stdout_identity.dev(), stdout_identity.ino())
    );
    assert_eq!(
        (returned_stderr.dev(), returned_stderr.ino()),
        (stderr_identity.dev(), stderr_identity.ino())
    );
    assert_eq!(returned_stdout.len(), 0);
    assert_eq!(returned_stderr.len(), 0);
    assert!(
        nix::fcntl::FdFlag::from_bits_truncate(
            nix::fcntl::fcntl(&stdout, nix::fcntl::FcntlArg::F_GETFD)
                .expect("re-read returned stdout flags")
        )
        .contains(nix::fcntl::FdFlag::FD_CLOEXEC)
    );
    assert!(
        !nix::fcntl::FdFlag::from_bits_truncate(
            nix::fcntl::fcntl(&stderr, nix::fcntl::FcntlArg::F_GETFD)
                .expect("re-read returned stderr flags")
        )
        .contains(nix::fcntl::FdFlag::FD_CLOEXEC)
    );
    assert!(
        !marker.exists(),
        "rejected request must not start its target"
    );
}

#[test]
fn strict_spool_keeps_stdout_and_stderr_separate() {
    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let result = exec_spooled_sync_cancellable(
        &request("printf out; printf err >&2", 32, 32),
        stdout,
        stderr,
        &AtomicBool::new(false),
    )
    .expect("spooled child completes");
    assert!(result.success, "{result:?}");
    assert_eq!(result.code, Some(0));
    assert_eq!(result.stdout_bytes, 3);
    assert_eq!(result.stderr_bytes, 3);
    assert!(result.cleanup.group_drained);
    assert!(result.capture.pipes.both_reached_eof());
    assert!(result.capture.complete);
    let mut stdout = result.stdout;
    let mut stderr = result.stderr;
    assert_eq!(read_file(&mut stdout).expect("read stdout spool"), b"out");
    assert_eq!(read_file(&mut stderr).expect("read stderr spool"), b"err");
}

#[test]
fn strict_spool_preserves_nonzero_exit_code() {
    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let result = exec_spooled_sync_cancellable(
        &request("exit 23", 0, 0),
        stdout,
        stderr,
        &AtomicBool::new(false),
    )
    .expect("nonzero child remains an observed result");
    assert_eq!(result.code, Some(23));
    assert!(!result.success);
    assert!(result.cleanup.group_drained);
    assert!(result.capture.pipes.both_reached_eof());
}

#[test]
fn strict_spool_retains_only_accepted_prefix_on_budget_overflow() {
    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let result = exec_spooled_sync_cancellable(
        &request("printf x >&2; printf 12345", 4, 8),
        stdout,
        stderr,
        &AtomicBool::new(false),
    )
    .expect("post-spawn capture failure is a result");
    assert!(!result.success);
    assert!(result.cleanup.group_drained);
    assert!(result.capture.pipes.both_reached_eof());
    assert!(!result.capture.complete);
    assert!(result.failures.iter().any(|failure| {
        failure.kind == SpoolFailureKind::Capture
            && failure.message.contains("stdout output exceeded")
    }));
    let mut stdout = result.stdout;
    let mut stderr = result.stderr;
    assert_eq!(
        read_file(&mut stdout).expect("read accepted stdout prefix"),
        b"1234"
    );
    assert_eq!(read_file(&mut stderr).expect("read stderr"), b"x");
}

#[test]
fn strict_spool_timeout_closes_group_and_keeps_cleanup_facts_separate() {
    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let request = ExecRequest::new("/bin/sleep", ["30"])
        .timeout(Duration::from_millis(50))
        .output_limits(8, 8);
    let result = exec_spooled_sync_cancellable(&request, stdout, stderr, &AtomicBool::new(false))
        .expect("timed-out child returns its cleanup outcome");
    assert!(result.execution.timed_out);
    assert!(!result.success);
    assert!(result.cleanup.group_drained);
    assert!(result.capture.pipes.both_reached_eof());
}

#[test]
fn strict_spool_cancellation_uses_one_finite_finalization() {
    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let cancellation = AtomicBool::new(true);
    let result = exec_spooled_sync_cancellable(
        &ExecRequest::new("/bin/sleep", ["30"]).output_limits(8, 8),
        stdout,
        stderr,
        &cancellation,
    )
    .expect("cancelled child returns a finite cleanup outcome");
    assert!(result.execution.cancelled);
    assert!(!result.success);
    assert!(result.cleanup.group_drained);
}

#[test]
fn absent_and_empty_stdin_keep_their_distinct_routing_contracts() {
    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let request = ExecRequest::new("/bin/sh", ["-c", ":"])
        .stdin_mode(crate::StdioMode::Inherit)
        .output_limits(0, 0);
    let result = exec_spooled_sync_cancellable(&request, stdout, stderr, &AtomicBool::new(false))
        .expect("inherited stdin is accepted");
    assert!(result.success);

    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let mut request = ExecRequest::new("/bin/sh", ["-c", ":"]).output_limits(0, 0);
    request.stdin = Some(Vec::new());
    let result = exec_spooled_sync_cancellable(&request, stdout, stderr, &AtomicBool::new(false))
        .expect("explicit empty stdin closes immediately");
    assert!(result.success);
}

#[test]
fn explicit_empty_stdin_is_a_piped_eof_line_protocol() {
    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let mut request = ExecRequest::new(
        "/bin/sh",
        [
            "-c",
            "if IFS= read -r line; then exit 91; else printf eof; fi",
        ],
    )
    .output_limits(8, 0);
    request.stdin = Some(Vec::new());
    let result = exec_spooled_sync_cancellable(&request, stdout, stderr, &AtomicBool::new(false))
        .expect("empty piped stdin is closed so the child reads EOF");
    assert!(result.success, "{result:?}");
    let mut stdout = result.stdout;
    assert_eq!(read_file(&mut stdout).expect("read EOF marker"), b"eof");
}

#[test]
fn inherited_piped_stdin_line_is_forwarded_by_the_child() {
    run_inherited_input_worker();
}

#[test]
fn worker_inherited_piped_stdin_line() {
    let worker_name = "spooled::tests::supported::worker_inherited_piped_stdin_line";
    if std::env::var(INHERITED_INPUT_WORKER_ENV).ok().as_deref() != Some(worker_name) {
        return;
    }

    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let request = ExecRequest::new(
        "/bin/sh",
        [
            "-c",
            "IFS= read -r line || exit 91; printf '%s\\n' \"$line\"",
        ],
    )
    .stdin_mode(crate::StdioMode::Inherit)
    .output_limits(128, 0)
    .timeout(Duration::from_secs(2));
    let result = exec_spooled_sync_cancellable(&request, stdout, stderr, &AtomicBool::new(false))
        .expect("child reads the worker's inherited piped stdin");
    assert!(result.success, "{result:?}");
    assert!(
        !result.execution.timed_out,
        "the inherited input line arrived in time"
    );
    let mut stdout = result.stdout;
    assert_eq!(
        read_file(&mut stdout).expect("read inherited line echo"),
        b"inherited-line\n"
    );
}

#[test]
fn invalid_request_returns_original_output_handles_before_spawn() {
    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let error = typed_error(
        exec_spooled_sync_cancellable(
            &ExecRequest::new("/bin/true", None::<&str>),
            stdout,
            stderr,
            &AtomicBool::new(false),
        )
        .expect_err("missing budgets must be rejected"),
    );
    assert_eq!(error.kind, SpoolFailureKind::Request);
    let (stdout, stderr, recovery) = error.into_parts();
    assert!(recovery.is_none());
    assert!(stdout.is_some());
    assert!(stderr.is_some());
}

#[test]
fn nonempty_stdin_is_a_named_pre_spawn_rejection() {
    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let mut request = ExecRequest::new("/bin/true", None::<&str>).output_limits(8, 8);
    request.stdin = Some(b"not supported".to_vec());
    let error = typed_error(
        exec_spooled_sync_cancellable(&request, stdout, stderr, &AtomicBool::new(false))
            .expect_err("nonempty strict stdin must be rejected"),
    );
    assert_eq!(error.kind, SpoolFailureKind::Request);
    assert!(error.message.contains("nonempty stdin"));
}

#[test]
fn unlinked_output_descriptor_is_rejected_by_link_count() {
    let stdout = unlinked_file();
    let stderr_files = output_pair();
    let stderr = stderr_files.stderr_file();
    let error = typed_error(
        exec_spooled_sync_cancellable(
            &request("/bin/true", 1, 1),
            stdout,
            stderr,
            &AtomicBool::new(false),
        )
        .expect_err("nlink-zero output must be rejected before spawn"),
    );
    assert_eq!(error.kind, SpoolFailureKind::Request);
    assert!(error.message.contains("one hard link"));
}

#[test]
fn same_inode_output_descriptors_are_rejected_without_reopening() {
    let spool_files = output_pair();
    let stdout = spool_files.stdout_file();
    let stderr = stdout.try_clone().expect("duplicate same file description");
    let error = typed_error(
        exec_spooled_sync_cancellable(
            &request("/bin/true", 1, 1),
            stdout,
            stderr,
            &AtomicBool::new(false),
        )
        .expect_err("aliased output handles must be rejected"),
    );
    assert_eq!(error.kind, SpoolFailureKind::Request);
    assert!(error.message.contains("distinct files"));
}

#[test]
fn append_mode_output_descriptor_is_rejected() {
    let spool_files = output_pair();
    let stdout = File::from(
        nix::fcntl::open(
            &spool_files.stdout_path(),
            nix::fcntl::OFlag::O_WRONLY
                | nix::fcntl::OFlag::O_APPEND
                | nix::fcntl::OFlag::O_CLOEXEC
                | nix::fcntl::OFlag::O_NOFOLLOW,
            nix::sys::stat::Mode::empty(),
        )
        .expect("open append descriptor with explicit flags"),
    );
    let stderr = spool_files.stderr_file();
    let error = typed_error(
        exec_spooled_sync_cancellable(
            &request("/bin/true", 1, 1),
            stdout,
            stderr,
            &AtomicBool::new(false),
        )
        .expect_err("append mode violates output accounting"),
    );
    assert_eq!(error.kind, SpoolFailureKind::Request);
    assert!(error.message.contains("append mode"));
}

#[test]
fn missing_target_after_anchor_returns_typed_spawn_error_and_custody() {
    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let request = ExecRequest::new("/jackin/does-not-exist", None::<&str>).output_limits(1, 1);
    let error = typed_error(
        exec_spooled_sync_cancellable(&request, stdout, stderr, &AtomicBool::new(false))
            .expect_err("missing target is a typed spawn error"),
    );
    assert_eq!(error.kind, SpoolFailureKind::Spawn);
    let (stdout, stderr, recovery) = error.into_parts();
    assert!(stdout.is_some());
    assert!(stderr.is_some());
    assert!(
        recovery.is_none(),
        "normal anchor cleanup should finish before error return"
    );
}

#[tokio::test]
async fn strict_spool_rejects_calls_inside_tokio_runtime() {
    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let error = typed_error(
        exec_spooled_sync_cancellable(
            &request("/bin/true", 1, 1),
            stdout,
            stderr,
            &AtomicBool::new(false),
        )
        .expect_err("strict sync call cannot block a Tokio runtime thread"),
    );
    assert_eq!(error.kind, SpoolFailureKind::Request);
}

#[test]
fn cancellation_flag_can_be_shared_with_a_controller() {
    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let cancellation = Arc::new(AtomicBool::new(false));
    let controller = Arc::clone(&cancellation);
    let thread = std::thread::spawn(move || {
        let cancel_at = Instant::now() + Duration::from_millis(30);
        while Instant::now() < cancel_at {
            owned_group::process_supervision_poll(Some(cancel_at));
        }
        controller.store(true, Ordering::Release);
    });
    let result = exec_spooled_sync_cancellable(
        &ExecRequest::new("/bin/sleep", ["30"]).output_limits(8, 8),
        stdout,
        stderr,
        &cancellation,
    )
    .expect("controller cancellation is observed");
    thread.join().expect("cancellation controller");
    assert!(result.execution.cancelled);
    assert!(result.cleanup.group_drained);
}

#[test]
fn cancellation_retains_output_emitted_before_the_causal_cancel() {
    let spool_files = output_pair();
    let (stdout, stderr) = spool_files.files();
    let output_path = spool_files.stdout_path();
    let cancellation = Arc::new(AtomicBool::new(false));
    let controller_flag = Arc::clone(&cancellation);
    let (observed_tx, observed_rx) = std::sync::mpsc::channel();
    let expected = b"before-cancel";
    let controller = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let captured = std::fs::metadata(&output_path)
                .is_ok_and(|metadata| metadata.len() >= expected.len() as u64);
            if captured {
                controller_flag.store(true, Ordering::Release);
                observed_tx
                    .send(true)
                    .expect("controller receiver remains active");
                return;
            }
            if Instant::now() >= deadline {
                controller_flag.store(true, Ordering::Release);
                observed_tx
                    .send(false)
                    .expect("controller receiver remains active");
                return;
            }
            owned_group::process_supervision_poll(Some(deadline));
        }
    });
    let result = exec_spooled_sync_cancellable(
        &request("printf before-cancel; exec /bin/sleep 30", 32, 0),
        stdout,
        stderr,
        &cancellation,
    )
    .expect("causally cancelled child returns the accepted prefix");
    controller
        .join()
        .expect("capture-triggered cancellation controller");
    assert!(observed_rx.recv().expect("controller reports observation"));
    assert!(result.execution.cancelled);
    assert!(!result.capture.complete);
    assert!(result.cleanup.group_drained);
    let mut stdout = result.stdout;
    assert_eq!(
        read_file(&mut stdout).expect("read retained pre-cancel bytes"),
        expected
    );
}

#[test]
fn exited_leader_cleanup_kills_pipe_holding_term_resistant_descendant() {
    let spool_files = output_pair();
    let fifo = spool_files.stdout_path().with_file_name("ready.fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("create a FIFO for the descendant readiness rendezvous");
    assert!(status.success(), "mkfifo status: {status}");

    let script = r#"(/bin/sh -c 'trap "" TERM; printf descendant-ready; printf "ready\n" > "$1"; exec /bin/sleep 30' jackin-descendant "$1") & IFS= read -r ready < "$1" || exit 90; printf parent-exit; exit 0"#;
    let mut request =
        ExecRequest::new("/bin/sh", ["-c", script, "jackin-parent"]).output_limits(128, 0);
    request.timeout = Some(Duration::from_secs(10));
    request.args.push(fifo.into_os_string());
    let (stdout, stderr) = spool_files.files();
    let result = exec_spooled_sync_cancellable(&request, stdout, stderr, &AtomicBool::new(false))
        .expect("leader exit triggers finite group cleanup while descendant holds stdout");
    assert_eq!(result.code, Some(0));
    assert!(
        !result.execution.timed_out,
        "FIFO readiness rendezvous completes in time"
    );
    assert!(result.cleanup.group_drained, "{result:?}");
    assert!(result.capture.pipes.both_reached_eof(), "{result:?}");
    assert!(result.capture.complete, "{result:?}");
    let mut stdout = result.stdout;
    let bytes = read_file(&mut stdout).expect("read leader and descendant output");
    assert!(
        bytes
            .windows(b"descendant-ready".len())
            .any(|w| w == b"descendant-ready")
    );
    assert!(
        bytes
            .windows(b"parent-exit".len())
            .any(|w| w == b"parent-exit")
    );
}

#[cfg(unix)]
#[test]
fn read_only_output_descriptor_is_rejected_before_anchor_spawn() {
    let spool_files = output_pair();
    let stdout = File::from(
        nix::fcntl::open(
            &spool_files.stdout_path(),
            nix::fcntl::OFlag::O_RDONLY
                | nix::fcntl::OFlag::O_CLOEXEC
                | nix::fcntl::OFlag::O_NOFOLLOW,
            nix::sys::stat::Mode::empty(),
        )
        .expect("open read-only spool descriptor with explicit flags"),
    );
    let stderr = spool_files.stderr_file();
    let error = typed_error(
        exec_spooled_sync_cancellable(
            &request("/bin/true", 1, 1),
            stdout,
            stderr,
            &AtomicBool::new(false),
        )
        .expect_err("read-only descriptor must be rejected"),
    );
    assert_eq!(error.kind, SpoolFailureKind::Request);
    assert!(error.message.contains("read-only"));
}

#[test]
fn partial_spool_write_failure_retains_written_prefix_and_fails_capture() {
    #[derive(Debug, Default)]
    struct FailsAfterTwo {
        bytes: Vec<u8>,
        failed: bool,
    }
    impl Write for FailsAfterTwo {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.failed {
                return Err(io::Error::other("injected spool write failure"));
            }
            let written = bytes.len().min(2);
            self.bytes.extend_from_slice(&bytes[..written]);
            self.failed = true;
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let mut sink = pump::StreamSink::new(FailsAfterTwo::default(), 8);
    let mut failures = Vec::new();
    assert!(pump::write_chunk(
        &mut sink,
        "stdout",
        b"four",
        &mut failures,
    ));
    assert_eq!(sink.writer.bytes, b"fo");
    assert_eq!(sink.accepted, 2, "count only the bytes the writer accepted");
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, SpoolFailureKind::Capture);
}

#[test]
fn missing_pipe_eof_records_a_stream_specific_capture_failure() {
    let mut failures = Vec::new();
    pump::record_missing_eof(&mut failures, "stdout", false);
    pump::record_missing_eof(&mut failures, "stderr", true);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, SpoolFailureKind::Capture);
    assert!(failures[0].message.contains("stdout"));
    assert!(failures[0].message.contains("did not reach EOF"));
}

#[derive(Debug)]
enum ReadAction {
    Bytes(Vec<u8>),
    Error(io::ErrorKind, &'static str),
    Eof,
}

#[derive(Debug)]
struct ScriptedReader {
    actions: VecDeque<ReadAction>,
}

impl ScriptedReader {
    fn new(actions: impl IntoIterator<Item = ReadAction>) -> Self {
        Self {
            actions: actions.into_iter().collect(),
        }
    }
}

impl tokio::io::AsyncRead for ScriptedReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match this.actions.pop_front() {
            Some(ReadAction::Bytes(bytes)) => {
                let count = bytes.len().min(buffer.remaining());
                buffer.put_slice(&bytes[..count]);
                if count < bytes.len() {
                    this.actions
                        .push_front(ReadAction::Bytes(bytes[count..].to_vec()));
                }
                Poll::Ready(Ok(()))
            }
            Some(ReadAction::Error(kind, message)) => {
                Poll::Ready(Err(io::Error::new(kind, message)))
            }
            Some(ReadAction::Eof) | None => Poll::Ready(Ok(())),
        }
    }
}

#[derive(Debug)]
struct FaultSpool {
    bytes: Vec<u8>,
    identity: SpoolIdentity,
    fail_after: Option<usize>,
    interrupt_after: Option<usize>,
    write_calls: usize,
    flush_fails: bool,
    sync_fails: bool,
    reported_length: Option<u64>,
}

impl FaultSpool {
    fn new(inode: u64) -> Self {
        Self {
            bytes: Vec::new(),
            identity: SpoolIdentity {
                device: 1,
                inode,
                uid: 1,
                links: 1,
                mode: 0o600,
            },
            fail_after: None,
            interrupt_after: None,
            write_calls: 0,
            flush_fails: false,
            sync_fails: false,
            reported_length: None,
        }
    }
}

impl Write for FaultSpool {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.write_calls += 1;
        if self
            .interrupt_after
            .is_some_and(|threshold| self.bytes.len() >= threshold)
        {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "injected repeated spool write interruption",
            ));
        }
        if let Some(limit) = self.fail_after {
            if self.bytes.len() >= limit {
                return Err(io::Error::other("injected spool write failure"));
            }
            let accepted = bytes.len().min(limit - self.bytes.len());
            self.bytes.extend_from_slice(&bytes[..accepted]);
            return Ok(accepted);
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.flush_fails {
            Err(io::Error::other("injected spool flush failure"))
        } else {
            Ok(())
        }
    }
}

impl OutputSpool for FaultSpool {
    fn sync_spool(&mut self) -> io::Result<()> {
        if self.sync_fails {
            Err(io::Error::other("injected spool sync failure"))
        } else {
            Ok(())
        }
    }

    fn spool_metadata(&self) -> io::Result<SpoolMetadata> {
        Ok(SpoolMetadata {
            regular_file: true,
            identity: self.identity,
            length: self
                .reported_length
                .unwrap_or_else(|| u64::try_from(self.bytes.len()).unwrap_or(u64::MAX)),
        })
    }
}

fn sleeping_group() -> owned_group::OwnedGroup {
    let mut command = std::process::Command::new("/bin/sleep");
    command
        .arg("30")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    owned_group::OwnedGroup::spawn(&mut command).expect("spawn owned test group")
}

async fn pump_scripted_streams(
    stdout_reader: ScriptedReader,
    stderr_reader: ScriptedReader,
    stdout_spool: &mut FaultSpool,
    stderr_spool: &mut FaultSpool,
) -> pump::PumpReport {
    let mut group = sleeping_group();
    let (actual_stdout, actual_stderr) = group.take_output_pipes();
    drop(actual_stdout);
    drop(actual_stderr);
    let cancellation = AtomicBool::new(false);
    pump::supervise_streams(
        group,
        pump::PumpInputs {
            stdout: Some(stdout_reader),
            stderr: Some(stderr_reader),
            failures: Vec::new(),
        },
        pump::PumpSinks {
            stdout: pump::StreamSink::new(stdout_spool, 32),
            stderr: pump::StreamSink::new(stderr_spool, 32),
        },
        pump::PumpControls {
            request_deadline: None,
            cancellation: &cancellation,
        },
    )
    .await
}

#[tokio::test]
async fn pump_interrupted_write_budget_preserves_prefix_and_runs_cleanup() {
    let mut stdout = FaultSpool::new(10);
    stdout.fail_after = Some(2);
    stdout.interrupt_after = Some(2);
    let mut stderr = FaultSpool::new(11);
    let report = pump_scripted_streams(
        ScriptedReader::new([ReadAction::Bytes(b"four".to_vec()), ReadAction::Eof]),
        ScriptedReader::new([ReadAction::Eof]),
        &mut stdout,
        &mut stderr,
    )
    .await;
    assert_eq!(stdout.bytes, b"fo");
    assert_eq!(report.stdout_accepted, 2);
    assert_eq!(stdout.write_calls, 1 + pump::WRITE_INTERRUPTION_BUDGET);
    assert!(report.cleanup.group_drained);
    assert!(report.pipes.both_reached_eof());
    assert!(report.target_status.is_some());
    assert!(report.failures.iter().any(|failure| {
        failure.kind == SpoolFailureKind::Capture
            && failure.message.contains("interruption budget exhausted")
    }));

    let mut failures = report.failures;
    let identities = (stdout.identity, stderr.identity);
    let (stdout_bytes, stderr_bytes) = finish_spools(
        &mut stdout,
        &mut stderr,
        identities,
        (report.stdout_accepted, report.stderr_accepted),
        (32, 32),
        &mut failures,
    );
    assert_eq!((stdout_bytes, stderr_bytes), (2, 0));
    assert!(
        !failures.is_empty(),
        "write failure remains a capture failure"
    );
}

#[tokio::test]
async fn pump_read_error_and_spool_finish_faults_keep_partial_data_and_cleanup_facts() {
    let mut stdout = FaultSpool::new(20);
    stdout.flush_fails = true;
    let mut stderr = FaultSpool::new(21);
    stderr.sync_fails = true;
    stderr.reported_length = Some(4);
    let report = pump_scripted_streams(
        ScriptedReader::new([
            ReadAction::Bytes(b"out".to_vec()),
            ReadAction::Error(
                io::ErrorKind::PermissionDenied,
                "injected pipe read failure",
            ),
        ]),
        ScriptedReader::new([ReadAction::Bytes(b"err".to_vec()), ReadAction::Eof]),
        &mut stdout,
        &mut stderr,
    )
    .await;
    assert_eq!(stdout.bytes, b"out");
    assert_eq!(stderr.bytes, b"err");
    assert_eq!((report.stdout_accepted, report.stderr_accepted), (3, 3));
    assert!(report.cleanup.group_drained);
    assert!(
        !report.pipes.both_reached_eof(),
        "stdout read error is not EOF"
    );
    assert!(report.failures.iter().any(|failure| {
        failure.kind == SpoolFailureKind::Capture && failure.message.contains("reading stdout pipe")
    }));
    assert!(report.failures.iter().any(|failure| {
        failure.kind == SpoolFailureKind::Capture
            && failure
                .message
                .contains("stdout output pipe did not reach EOF")
    }));

    let mut failures = report.failures;
    let identities = (stdout.identity, stderr.identity);
    let (stdout_bytes, stderr_bytes) = finish_spools(
        &mut stdout,
        &mut stderr,
        identities,
        (report.stdout_accepted, report.stderr_accepted),
        (32, 32),
        &mut failures,
    );
    assert_eq!((stdout_bytes, stderr_bytes), (3, 4));
    assert!(failures.iter().any(|failure| {
        failure.kind == SpoolFailureKind::Capture
            && failure.message.contains("flushing stdout spool")
    }));
    assert!(failures.iter().any(|failure| {
        failure.kind == SpoolFailureKind::Capture
            && failure.message.contains("syncing stderr spool")
    }));
    assert!(failures.iter().any(|failure| {
        failure.kind == SpoolFailureKind::Capture
            && failure.message.contains("stderr spool byte accounting")
    }));
}

#[derive(Debug, Default)]
struct GroupObservation {
    anchor_registered_at_signal: bool,
    anchor_released_before_probe: bool,
    signal_calls: usize,
    target_wait_calls: usize,
    anchor_wait_calls: usize,
    probes: usize,
    signals_after_release: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupMode {
    ReuseThenAbsent,
    PersistentOccupancy,
    SignalPermissionDenied,
    AnchorWaitInterruptedOnce,
    AnchorWaitEchild,
    TargetWaitInterruptedOnce,
    TargetWaitInterruptedDuringCleanup,
    TargetWaitEchild,
    TargetWaitHardError,
    SignalDeniedWithTargetWaitError,
}

#[derive(Debug)]
struct ObservedGroupBackend {
    observation: Arc<std::sync::Mutex<GroupObservation>>,
    mode: GroupMode,
    anchor_wait_interrupted: bool,
    target_wait_interrupted: bool,
    signal_attempted: bool,
}

impl owned_group::GroupSignalBackend for ObservedGroupBackend {
    fn signal_group(
        &mut self,
        group: nix::unistd::Pid,
        signal: nix::sys::signal::Signal,
    ) -> nix::Result<()> {
        let mut observation = self
            .observation
            .lock()
            .map_err(|_| nix::errno::Errno::EIO)?;
        observation.signal_calls += 1;
        if observation.anchor_released_before_probe {
            observation.signals_after_release += 1;
            return Err(nix::errno::Errno::ESRCH);
        }
        observation.anchor_registered_at_signal = crate::coordinate_children(|registry| {
            u32::try_from(group.as_raw()).is_ok_and(|pid| registry.contains(pid))
        });
        drop(observation);
        self.signal_attempted = true;
        if matches!(
            self.mode,
            GroupMode::SignalPermissionDenied
                | GroupMode::AnchorWaitEchild
                | GroupMode::SignalDeniedWithTargetWaitError
        ) {
            return Err(nix::errno::Errno::EPERM);
        }
        nix::sys::signal::killpg(group, signal)
    }

    fn group_exists(&mut self, _group: nix::unistd::Pid) -> nix::Result<()> {
        let mut observation = self
            .observation
            .lock()
            .map_err(|_| nix::errno::Errno::EIO)?;
        observation.anchor_released_before_probe = true;
        observation.probes += 1;
        match self.mode {
            GroupMode::ReuseThenAbsent if observation.probes > 1 => Err(nix::errno::Errno::ESRCH),
            GroupMode::SignalPermissionDenied | GroupMode::AnchorWaitEchild => {
                Err(nix::errno::Errno::ESRCH)
            }
            GroupMode::AnchorWaitInterruptedOnce
            | GroupMode::TargetWaitInterruptedOnce
            | GroupMode::TargetWaitInterruptedDuringCleanup
            | GroupMode::TargetWaitEchild
            | GroupMode::TargetWaitHardError
            | GroupMode::SignalDeniedWithTargetWaitError => Err(nix::errno::Errno::ESRCH),
            GroupMode::ReuseThenAbsent | GroupMode::PersistentOccupancy => Ok(()),
        }
    }

    fn wait_child(
        &mut self,
        role: owned_group::OwnedChild,
        child: &mut std::process::Child,
    ) -> io::Result<Option<std::process::ExitStatus>> {
        use owned_group::OwnedChild;

        let mut observation = self
            .observation
            .lock()
            .map_err(|_| io::Error::other("fixture observation mutex poisoned"))?;
        match role {
            OwnedChild::Target => observation.target_wait_calls += 1,
            OwnedChild::Anchor => observation.anchor_wait_calls += 1,
        }
        drop(observation);

        match (self.mode, role) {
            (GroupMode::AnchorWaitEchild, OwnedChild::Anchor) => Err(io::Error::from_raw_os_error(
                nix::errno::Errno::ECHILD as i32,
            )),
            (GroupMode::AnchorWaitInterruptedOnce, OwnedChild::Anchor)
                if !self.anchor_wait_interrupted =>
            {
                self.anchor_wait_interrupted = true;
                Err(io::Error::from_raw_os_error(
                    nix::errno::Errno::EINTR as i32,
                ))
            }
            (GroupMode::TargetWaitInterruptedOnce, OwnedChild::Target)
                if !self.target_wait_interrupted =>
            {
                self.target_wait_interrupted = true;
                Err(io::Error::from_raw_os_error(
                    nix::errno::Errno::EINTR as i32,
                ))
            }
            (GroupMode::TargetWaitInterruptedDuringCleanup, OwnedChild::Target)
                if self.signal_attempted && !self.target_wait_interrupted =>
            {
                self.target_wait_interrupted = true;
                Err(io::Error::from_raw_os_error(
                    nix::errno::Errno::EINTR as i32,
                ))
            }
            (GroupMode::TargetWaitEchild, OwnedChild::Target) => Err(io::Error::from_raw_os_error(
                nix::errno::Errno::ECHILD as i32,
            )),
            (
                GroupMode::TargetWaitHardError | GroupMode::SignalDeniedWithTargetWaitError,
                OwnedChild::Target,
            ) if self.mode != GroupMode::SignalDeniedWithTargetWaitError
                || self.signal_attempted =>
            {
                Err(io::Error::from_raw_os_error(nix::errno::Errno::EIO as i32))
            }
            _ => child.try_wait(),
        }
    }
}

fn started_exited_group(
    mode: GroupMode,
) -> (
    owned_group::OwnedGroup,
    Arc<std::sync::Mutex<GroupObservation>>,
) {
    let mut command = std::process::Command::new("/bin/sh");
    command
        .args(["-c", "exit 0"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let (mut owned, observation) = spawn_observed_group(mode, &mut command);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match owned.observe_target_exit() {
            Ok(true) => break,
            Ok(false) => {
                assert!(Instant::now() < deadline, "test target did not exit");
                owned_group::process_supervision_poll(Some(deadline));
            }
            Err(error) => panic!("observe test target: {error}"),
        }
    }
    (owned, observation)
}

fn spawn_observed_group(
    mode: GroupMode,
    command: &mut std::process::Command,
) -> (
    owned_group::OwnedGroup,
    Arc<std::sync::Mutex<GroupObservation>>,
) {
    let observation = Arc::new(std::sync::Mutex::new(GroupObservation::default()));
    let backend = ObservedGroupBackend {
        observation: Arc::clone(&observation),
        mode,
        anchor_wait_interrupted: false,
        target_wait_interrupted: false,
        signal_attempted: false,
    };
    let group = owned_group::OwnedGroup::spawn_with_backend(command, Box::new(backend))
        .expect("spawn owned group with local observation backend");
    (group, observation)
}

fn signal_once_then_expire_for_test(
    mut group: owned_group::OwnedGroup,
    mut failures: Vec<SpoolFailure>,
) -> owned_group::CleanupReport {
    group.begin_finalization();
    group.cleanup_step(&mut failures);
    group.expire_cleanup_deadline_for_test();
    group.cleanup_step(&mut failures);
    assert!(
        group.cleanup_complete(),
        "forced finite cleanup is complete"
    );
    group.into_report(failures)
}

#[test]
fn reused_group_identity_is_only_observed_after_anchor_release() {
    let (group, observation) = started_exited_group(GroupMode::ReuseThenAbsent);
    let cleanup = group.cleanup_without_pump();
    assert!(cleanup.group_drained);
    assert_eq!(
        cleanup.target_status.and_then(|status| status.code()),
        Some(0)
    );
    let observation = observation.lock().expect("read local group observation");
    assert!(observation.anchor_registered_at_signal);
    assert!(observation.anchor_released_before_probe);
    assert_eq!(observation.signals_after_release, 0);
}

#[test]
fn persistent_reused_group_returns_uncertain_without_foreign_signal() {
    let (group, observation) = started_exited_group(GroupMode::PersistentOccupancy);
    let cleanup = group.cleanup_without_pump();
    assert!(!cleanup.group_drained);
    assert!(cleanup.failures.iter().any(|failure| {
        failure.kind == SpoolFailureKind::Cleanup
            && failure.message.contains("cleanup deadline expired")
    }));
    let observation = observation.lock().expect("read local group observation");
    assert!(observation.anchor_released_before_probe);
    assert_eq!(observation.signals_after_release, 0);
}

#[test]
fn permanent_group_signal_error_is_recorded_once_and_closes_authority() {
    let (group, observation) = started_exited_group(GroupMode::SignalPermissionDenied);
    let mut cleanup = group.cleanup_without_pump();
    assert!(
        cleanup.group_drained,
        "anchor natural exit is independently confirmed"
    );
    assert!(
        cleanup.recovery.is_none(),
        "both direct children were consumed"
    );
    assert_eq!(
        cleanup
            .failures
            .iter()
            .filter(|failure| failure.message.contains("SIGKILL error"))
            .count(),
        1
    );
    let observation = observation.lock().expect("read local group observation");
    assert_eq!(observation.signal_calls, 1);
    assert_eq!(observation.signals_after_release, 0);
    drop(cleanup.recovery.take());
}

#[test]
fn anchor_wait_interruption_retries_on_a_later_finite_cleanup_transition() {
    let (group, _observation) = started_exited_group(GroupMode::AnchorWaitInterruptedOnce);
    let cleanup = group.cleanup_without_pump();
    assert!(cleanup.group_drained);
    assert!(cleanup.recovery.is_none());
    assert!(
        cleanup.failures.is_empty(),
        "EINTR is retried without inventing uncertainty"
    );
}

#[test]
fn target_wait_interruption_retries_and_observes_terminal_status() {
    let (group, observation) = started_exited_group(GroupMode::TargetWaitInterruptedOnce);
    let cleanup = group.cleanup_without_pump();
    assert_eq!(
        cleanup.target_status.and_then(|status| status.code()),
        Some(0)
    );
    assert!(cleanup.group_drained);
    assert!(cleanup.recovery.is_none());
    assert!(cleanup.failures.is_empty());
    assert!(
        observation
            .lock()
            .expect("read injected target wait observation")
            .target_wait_calls
            >= 2,
        "EINTR must defer observation to a later finite transition"
    );
}

#[test]
fn target_wait_interruption_after_group_signal_retries_without_false_uncertainty() {
    let mut command = std::process::Command::new("/bin/sh");
    command
        .args(["-c", "exit 0"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let (group, observation) =
        spawn_observed_group(GroupMode::TargetWaitInterruptedDuringCleanup, &mut command);
    let cleanup = group.cleanup_without_pump();
    assert!(cleanup.target_status.is_some());
    assert!(cleanup.group_drained);
    assert!(cleanup.recovery.is_none());
    assert!(cleanup.failures.is_empty());
    let observation = observation
        .lock()
        .expect("read post-signal target wait observations");
    assert_eq!(observation.signal_calls, 1);
    assert!(observation.target_wait_calls >= 2);
}

#[test]
fn target_wait_echild_keeps_status_and_direct_child_custody_uncertain() {
    target_wait_failure_keeps_custody(GroupMode::TargetWaitEchild, nix::errno::Errno::ECHILD);
}

#[test]
fn target_wait_hard_error_keeps_status_and_direct_child_custody_uncertain() {
    target_wait_failure_keeps_custody(GroupMode::TargetWaitHardError, nix::errno::Errno::EIO);
}

fn target_wait_failure_keeps_custody(mode: GroupMode, expected: nix::errno::Errno) {
    let mut command = std::process::Command::new("/bin/sleep");
    command
        .arg("30")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let (mut group, observation) = spawn_observed_group(mode, &mut command);
    let target_pid = group.target_pid();
    let anchor_pid = u32::try_from(group.process_group_id()).expect("positive anchor PID");
    let error = group
        .observe_target_exit()
        .expect_err("injected target wait error prevents status inference");
    assert_eq!(error.raw_os_error(), Some(expected as i32));
    let cleanup = signal_once_then_expire_for_test(
        group,
        vec![SpoolFailure {
            kind: SpoolFailureKind::Execution,
            message: format!("observing requested child exit: {error}"),
        }],
    );
    assert!(cleanup.target_status.is_none());
    assert!(!cleanup.group_drained);
    assert!(cleanup.failures.iter().any(|failure| {
        failure.kind == SpoolFailureKind::Cleanup
            && failure.message.contains("cleanup deadline expired")
    }));
    let mut recovery = cleanup.recovery.expect("retain both uncertain children");
    assert_eq!(recovery.retained_children(), 2);
    let observation = observation.lock().expect("read target wait observations");
    assert_eq!(observation.signal_calls, 1);
    assert_eq!(observation.target_wait_calls, 1);
    assert_eq!(observation.anchor_wait_calls, 0);
    drop(observation);

    recovery
        .terminate_children_for_test()
        .expect("test cleanup uses retained direct-child handles");
    assert_eq!(recovery.retained_children(), 0);
    for child_pid in [target_pid, anchor_pid] {
        let pid = nix::unistd::Pid::from_raw(i32::try_from(child_pid).expect("child PID fits"));
        assert_eq!(
            nix::sys::wait::waitpid(pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG))
                .expect_err("test cleanup consumed child status through its owned handle"),
            nix::errno::Errno::ECHILD
        );
    }
}

#[test]
fn signal_error_with_target_wait_error_retains_unresolved_live_custody() {
    let mut command = std::process::Command::new("/bin/sleep");
    command
        .arg("30")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let (mut group, observation) =
        spawn_observed_group(GroupMode::SignalDeniedWithTargetWaitError, &mut command);
    let target_pid = group.target_pid();
    let anchor_pid = u32::try_from(group.process_group_id()).expect("positive anchor PID");
    assert!(
        !group
            .observe_target_exit()
            .expect("target remains live before cleanup")
    );

    let cleanup = signal_once_then_expire_for_test(group, Vec::new());
    assert!(cleanup.target_status.is_none());
    assert!(!cleanup.group_drained);
    assert!(cleanup.failures.iter().any(|failure| {
        failure.kind == SpoolFailureKind::Cleanup && failure.message.contains("SIGKILL error")
    }));
    assert!(cleanup.failures.iter().any(|failure| {
        failure.kind == SpoolFailureKind::Cleanup
            && failure
                .message
                .contains("observing requested child after group closure")
    }));
    assert!(cleanup.failures.iter().any(|failure| {
        failure.kind == SpoolFailureKind::Cleanup
            && failure.message.contains("cleanup deadline expired")
    }));
    let mut recovery = cleanup
        .recovery
        .expect("retain unresolved direct-child custody");
    assert_eq!(recovery.retained_children(), 2);
    let observation = observation
        .lock()
        .expect("read combined signal and target wait observations");
    assert_eq!(observation.signal_calls, 1);
    assert_eq!(observation.target_wait_calls, 2);
    assert_eq!(observation.anchor_wait_calls, 0);
    assert_eq!(observation.signals_after_release, 0);
    drop(observation);

    recovery
        .terminate_children_for_test()
        .expect("test cleanup uses retained direct-child handles");
    assert_eq!(recovery.retained_children(), 0);
    for child_pid in [target_pid, anchor_pid] {
        let pid = nix::unistd::Pid::from_raw(i32::try_from(child_pid).expect("child PID fits"));
        assert_eq!(
            nix::sys::wait::waitpid(pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG))
                .expect_err("test cleanup consumed child status through its owned handle"),
            nix::errno::Errno::ECHILD
        );
    }
}

#[test]
fn permanent_anchor_echild_retains_direct_child_recovery_custody() {
    let (group, observation) = started_exited_group(GroupMode::AnchorWaitEchild);
    let anchor_pid = nix::unistd::Pid::from_raw(group.process_group_id());
    let mut cleanup = group.cleanup_without_pump();
    assert!(!cleanup.group_drained);
    assert!(
        cleanup
            .recovery
            .as_ref()
            .is_some_and(|recovery| { recovery.retained_children() == 1 })
    );
    assert!(cleanup.failures.iter().any(|failure| {
        failure.kind == SpoolFailureKind::Cleanup
            && failure.message.contains("observing owned anchor")
    }));
    let observation = observation.lock().expect("read injected wait observation");
    assert_eq!(observation.signal_calls, 1);
    assert_eq!(observation.signals_after_release, 0);
    drop(observation);

    let recovery = cleanup.recovery.as_mut().expect("retain uncertain anchor");
    let polls = recovery.poll_reap_nonblocking();
    assert_eq!(polls.len(), 1);
    assert!(polls[0].as_ref().is_ok_and(|reaped| *reaped));
    assert_eq!(recovery.retained_children(), 0);
    assert_eq!(
        nix::sys::wait::waitpid(anchor_pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG))
            .expect_err("recovery custody consumed the anchor status"),
        nix::errno::Errno::ECHILD
    );
}

#[test]
fn observer_panic_drops_a_fully_registered_owner_without_delayed_signal() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::process::Command;

    let mut command = Command::new("/bin/sleep");
    command
        .arg("30")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut group = owned_group::OwnedGroup::spawn(&mut command)
        .expect("spawn owner before observer notification");
    assert!(!group.observe_target_exit().expect("observe live target"));
    let group_pid = group.process_group_id();
    let anchor_pid = u32::try_from(group.process_group_id()).expect("positive anchor PID");
    let target_pid = group.target_pid();
    assert!(crate::coordinate_children(|registry| {
        registry.contains(anchor_pid) && registry.contains(target_pid)
    }));

    let panic = catch_unwind(AssertUnwindSafe(move || {
        let _fully_owned = group;
        panic!("injected post-ownership observer panic");
    }));
    assert!(panic.is_err());
    assert!(!crate::coordinate_children(|registry| {
        registry.contains(anchor_pid) || registry.contains(target_pid)
    }));

    for child_pid in [anchor_pid, target_pid] {
        let pid = nix::unistd::Pid::from_raw(i32::try_from(child_pid).expect("child PID fits i32"));
        assert_eq!(
            nix::sys::wait::waitpid(pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG))
                .expect_err("active Drop consumed both live-target child statuses"),
            nix::errno::Errno::ECHILD
        );
    }
    assert_eq!(
        nix::sys::signal::killpg(nix::unistd::Pid::from_raw(group_pid), None)
            .expect_err("active Drop removed the live process group"),
        nix::errno::Errno::ESRCH
    );
}
