// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::fmt;

#[test]
fn captured_process_future_keeps_io_buffers_off_the_caller_stack() {
    let request = ExecRequest::new("not-executed", None::<&str>);
    let future = exec_async(&request);
    let bytes = size_of_val(&future);
    assert!(
        bytes <= 4096,
        "process future is {bytes} bytes; budget is 4096"
    );
}

#[tokio::test]
async fn true_succeeds() {
    let result = exec_async(&ExecRequest::new("true", None::<&str>))
        .await
        .unwrap();
    assert!(result.success);
    assert!(!result.timed_out);
    assert!(result.stdout.is_empty());
}

#[tokio::test]
async fn false_fails_without_retry() {
    let result = exec_async(&ExecRequest::new("false", None::<&str>))
        .await
        .unwrap();
    assert!(!result.success);
    assert_eq!(result.code, Some(1));
}

#[tokio::test]
async fn capture_echo_stdout() {
    let out = capture_stdout_async(&ExecRequest::new("echo", ["hello-transport"]))
        .await
        .unwrap();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("hello-transport"), "{s}");
}

#[tokio::test]
async fn timeout_fires_on_sleep() {
    let result = exec_async(&ExecRequest::new("sleep", ["5"]).timeout(Duration::from_millis(50)))
        .await
        .unwrap();
    assert!(result.timed_out, "expected timeout, got {result:?}");
    assert!(!result.success);
}

#[tokio::test]
async fn no_timeout_waits_for_fast_command() {
    let result = exec_async(&ExecRequest::new("true", None::<&str>).no_timeout())
        .await
        .unwrap();
    assert!(result.success);
    assert!(!result.timed_out);
}

#[tokio::test]
async fn retry_eventually_succeeds() {
    let result = exec_async(&ExecRequest::new("false", None::<&str>).retry(RetryPolicy {
        max_retries: 2,
        delay: Duration::from_millis(1),
    }))
    .await
    .unwrap();
    assert!(!result.success);
}

#[test]
fn sync_facade_runs_true() {
    let result = exec_sync(&ExecRequest::new("true", None::<&str>)).unwrap();
    assert!(result.success);
}

#[test]
fn capture_stdout_sync_echo() {
    let out = capture_stdout_sync(&ExecRequest::new("printf", ["ok"])).unwrap();
    assert_eq!(out, b"ok");
}

#[test]
fn environment_clear_remove_and_add_are_applied() {
    let request = ExecRequest::new("sh", ["-c", "printf '%s:%s' \"$KEPT\" \"$REMOVED\""])
        .env_clear()
        .envs([("KEPT", "yes"), ("REMOVED", "no")])
        .env_remove(["REMOVED"]);
    let out = capture_stdout_sync(&request).unwrap();
    assert_eq!(out, b"yes:");
}

#[test]
fn sync_spawn_exposes_captured_child_lifecycle() {
    let request = ExecRequest::new("printf", ["spawned"])
        .stdout_mode(StdioMode::Capture)
        .stderr_mode(StdioMode::Null);
    let output = spawn_sync(&request).unwrap().wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"spawned");
}

#[tokio::test]
async fn async_spawn_exposes_captured_child_lifecycle() {
    let request = ExecRequest::new("printf", ["spawned-async"])
        .stdout_mode(StdioMode::Capture)
        .stderr_mode(StdioMode::Null);
    let output = spawn_async(&request)
        .unwrap()
        .wait_with_output()
        .await
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"spawned-async");
}

#[cfg(unix)]
#[tokio::test]
async fn timeout_covers_a_blocked_stdin_write_and_reaps_child() {
    let path = std::env::temp_dir().join(format!("jackin-process-blocked-{}", std::process::id()));
    let mut request = ExecRequest::new(
        "sh",
        [
            "-c",
            "printf '%s' $$ > \"$1\"; exec sleep 30",
            "fixture",
            path.to_str().unwrap(),
        ],
    )
    .timeout(Duration::from_millis(150));
    request.stdin = Some(vec![b'x'; 2 * 1024 * 1024]);
    let outcome = tokio::time::timeout(Duration::from_secs(2), exec_async(&request)).await;
    let pid: u32 = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    let absent = !process_exists(pid);
    if !absent {
        drop(
            std::process::Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .status(),
        );
    }
    drop(std::fs::remove_file(path));
    let result = outcome
        .expect("request timeout must cover blocked stdin")
        .unwrap();
    assert!(result.timed_out, "{result:?}");
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
    assert!(absent, "fixture child {pid} must be reaped before return");
}

#[cfg(unix)]
#[tokio::test]
async fn stdin_and_output_progress_concurrently() {
    let mut request = ExecRequest::new(
        "sh",
        [
            "-c",
            "dd if=/dev/zero bs=1024 count=256 2>/dev/null; cat >/dev/null",
        ],
    )
    .timeout(Duration::from_millis(500));
    request.stdin = Some(vec![b'x'; 2 * 1024 * 1024]);
    let result = tokio::time::timeout(Duration::from_secs(2), exec_async(&request))
        .await
        .expect("stdin and stdout must not deadlock")
        .unwrap();
    assert!(result.success, "{result:?}");
    assert!(!result.timed_out);
    assert_eq!(result.stdout, vec![0; 256 * 1024]);
    assert!(result.stderr.is_empty());
}

#[tokio::test]
async fn bounded_capture_accepts_exact_independent_limits() {
    let result = exec_async(
        &ExecRequest::new("sh", ["-c", "printf 1234; printf abc >&2"]).output_limits(4, 3),
    )
    .await
    .unwrap();
    assert!(result.success);
    assert_eq!(result.stdout, b"1234");
    assert_eq!(result.stderr, b"abc");
    let empty = exec_async(&ExecRequest::new("true", None::<&str>).output_limits(0, 0))
        .await
        .unwrap();
    assert!(empty.success);
    assert!(empty.stdout.is_empty());
    assert!(empty.stderr.is_empty());
}

#[tokio::test]
async fn limits_require_capture_and_exec_ownership() {
    for mode in [StdioMode::Null, StdioMode::Inherit] {
        let stdout = ExecRequest::new("missing-fixture-program", None::<&str>)
            .output_limits(1, 1)
            .stdout_mode(mode);
        let stderr = ExecRequest::new("missing-fixture-program", None::<&str>)
            .output_limits(1, 1)
            .stderr_mode(mode);
        assert!(
            exec_async(&stdout)
                .await
                .unwrap_err()
                .to_string()
                .contains("stdout byte limit requires Capture")
        );
        assert!(
            exec_async(&stderr)
                .await
                .unwrap_err()
                .to_string()
                .contains("stderr byte limit requires Capture")
        );
    }
    let impossible =
        ExecRequest::new("missing-fixture-program", None::<&str>).output_limits(usize::MAX, 1);
    assert!(
        exec_async(&impossible)
            .await
            .unwrap_err()
            .to_string()
            .contains("stdout byte limit exceeds maximum buffer size")
    );
    let request = ExecRequest::new("missing-fixture-program", None::<&str>).output_limits(1, 1);
    assert!(
        spawn_sync(&request)
            .unwrap_err()
            .to_string()
            .contains("cannot enforce")
    );
    assert!(
        spawn_async(&request)
            .unwrap_err()
            .to_string()
            .contains("cannot enforce")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn oversized_streams_fail_promptly_and_reap_child() {
    for (name, redirect) in [("stdout", ""), ("stderr", " >&2")] {
        let path = std::env::temp_dir().join(format!(
            "jackin-process-overflow-{}-{name}",
            std::process::id()
        ));
        let script = format!("printf '%s' $$ > \"$1\"; printf 12345{redirect}; exec sleep 30");
        let request = ExecRequest::new("sh", ["-c", &script, "fixture", path.to_str().unwrap()])
            .output_limits(4, 4)
            .timeout(Duration::from_secs(5));
        let outcome = tokio::time::timeout(Duration::from_secs(2), exec_async(&request)).await;
        let pid: u32 = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        let absent = !process_exists(pid);
        if !absent {
            drop(
                std::process::Command::new("kill")
                    .args(["-KILL", &pid.to_string()])
                    .status(),
            );
        }
        drop(std::fs::remove_file(path));
        let error = outcome
            .expect("overflow must terminate without waiting for timeout")
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&format!("{name} exceeded byte limit 4")),
            "{error:#}"
        );
        assert!(
            absent,
            "overflow fixture child {pid} must be reaped before return"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn default_capture_budget_is_finite() {
    let request = ExecRequest::new(
        "sh",
        ["-c", "dd if=/dev/zero bs=1048576 count=17 2>/dev/null"],
    )
    .timeout(Duration::from_secs(5));
    match exec_async(&request).await {
        Err(error) => assert!(
            error
                .to_string()
                .contains("stdout exceeded byte limit 16777216"),
            "{error:#}"
        ),
        Ok(result) => panic!(
            "default capture must reject overflow: success={}, stdout_bytes={}",
            result.success,
            result.stdout.len()
        ),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn timeout_owns_descendants_after_direct_child_exits() {
    let path =
        std::env::temp_dir().join(format!("jackin-process-descendant-{}", std::process::id()));
    let script = "exec 3<&0; sleep 30 <&3 & printf '%s:%s' $$ $! > \"$1\"; exit 0";
    let mut request = ExecRequest::new("sh", ["-c", script, "fixture", path.to_str().unwrap()])
        .timeout(Duration::from_millis(150));
    request.stdin = Some(vec![b'x'; 2 * 1024 * 1024]);
    let outcome = tokio::time::timeout(Duration::from_secs(2), exec_async(&request)).await;
    let pids = std::fs::read_to_string(&path).unwrap();
    let (parent, descendant) = pids.split_once(':').unwrap();
    let parent: u32 = parent.parse().unwrap();
    let descendant: u32 = descendant.parse().unwrap();
    let parent_absent = !process_exists(parent);
    let mut descendant_absent = !process_exists(descendant);
    for _ in 0..100 {
        if descendant_absent {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        descendant_absent = !process_exists(descendant);
    }
    if !descendant_absent {
        drop(
            std::process::Command::new("kill")
                .args(["-KILL", &descendant.to_string()])
                .status(),
        );
    }
    drop(std::fs::remove_file(path));
    let result = outcome
        .expect("deadline includes writer and inherited pipes")
        .unwrap();
    assert!(result.timed_out, "{result:?}");
    assert!(!result.success);
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
    assert!(
        parent_absent,
        "direct child {parent} must be reaped before return"
    );
    assert!(
        descendant_absent,
        "owned descendant {descendant} must be killed"
    );
}

#[cfg(unix)]
struct FixtureDir {
    path: PathBuf,
    cleanup_safe: bool,
}

#[cfg(unix)]
impl FixtureDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        for _ in 0..64 {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("jackin-process-stdin-{}-{id}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    return Self {
                        path,
                        cleanup_safe: false,
                    };
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create owned fixture directory: {error}"),
            }
        }
        panic!("could not create an owned process fixture directory after 64 attempts");
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn confirm_child_absent(&mut self) {
        self.cleanup_safe = true;
    }
}

#[cfg(unix)]
impl Drop for FixtureDir {
    fn drop(&mut self) {
        if self.cleanup_safe {
            drop(std::fs::remove_dir_all(&self.path));
        }
    }
}

#[cfg(unix)]
enum FixtureCustody {
    MissingPidRecord,
    InvalidPidRecord(String),
    ChildStillPresent(u32),
    ChildAbsent(u32),
}

#[cfg(unix)]
impl fmt::Debug for FixtureCustody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPidRecord => formatter.debug_tuple("MissingPidRecord").finish(),
            Self::InvalidPidRecord(raw_record) => formatter
                .debug_struct("InvalidPidRecord")
                .field("record_bytes", &raw_record.len())
                .finish(),
            Self::ChildStillPresent(pid) => formatter
                .debug_tuple("ChildStillPresent")
                .field(pid)
                .finish(),
            Self::ChildAbsent(pid) => formatter.debug_tuple("ChildAbsent").field(pid).finish(),
        }
    }
}

#[cfg(unix)]
async fn finalize_fixture(dir: &mut FixtureDir, pid_path: &std::path::Path) -> FixtureCustody {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    let mut invalid_record = None;
    loop {
        if let Ok(text) = std::fs::read_to_string(pid_path) {
            match text.parse::<u32>() {
                Ok(pid) => {
                    invalid_record = None;
                    if !process_exists(pid) {
                        dir.confirm_child_absent();
                        return FixtureCustody::ChildAbsent(pid);
                    }
                    if tokio::time::Instant::now() >= deadline {
                        return FixtureCustody::ChildStillPresent(pid);
                    }
                }
                Err(_) => invalid_record = Some(text),
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return invalid_record.map_or(
                FixtureCustody::MissingPidRecord,
                FixtureCustody::InvalidPidRecord,
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[cfg(unix)]
fn finalize_fixture_sync(dir: &mut FixtureDir, pid_path: &std::path::Path) -> FixtureCustody {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("build bounded fixture-finalization runtime");
    runtime.block_on(finalize_fixture(dir, pid_path))
}

#[cfg(unix)]
fn assert_fixture_absent(dir: &FixtureDir, custody: &FixtureCustody) {
    assert!(
        matches!(custody, FixtureCustody::ChildAbsent(_)),
        "fixture child absence is unproven: {custody:?}; fixture evidence retained at {}",
        dir.path().display()
    );
}

#[cfg(unix)]
async fn wait_for_fixture_ready(path: PathBuf) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        if path.is_file() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow::anyhow!(
                "fixture did not close stdin and signal readiness within 1s"
            ));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[cfg(unix)]
fn fixture_paths(dir: &FixtureDir) -> (PathBuf, PathBuf) {
    (
        dir.path().join("child.pid"),
        dir.path().join("stdin-closed"),
    )
}

#[cfg(unix)]
fn assert_broken_pipe(error: &StdinDeliveryError) {
    use std::error::Error as _;

    let source = error
        .source()
        .expect("delivery error retains its I/O source")
        .downcast_ref::<io::Error>()
        .expect("delivery error source is the original I/O error");
    assert_eq!(source.kind(), io::ErrorKind::BrokenPipe);
}

#[cfg(unix)]
fn observed_delivery_error<'a>(
    error: &'a anyhow::Error,
    dir: &FixtureDir,
    custody: &FixtureCustody,
) -> &'a StdinDeliveryError {
    error.downcast_ref::<StdinDeliveryError>().unwrap_or_else(|| {
        panic!(
            "expected typed stdin delivery error, got {error:#}; custody={custody:?}; evidence={}",
            dir.path().display()
        )
    })
}

#[cfg(unix)]
#[tokio::test]
async fn deadline_preserves_child_diagnostics_after_stdin_broken_pipe() {
    let mut dir = FixtureDir::new();
    let (pid_path, ready_path) = fixture_paths(&dir);
    let script = concat!(
        "printf '%s' $$ > \"$1\"; ",
        "exec 0<&-; : > \"$2\"; ",
        "printf 'stdin-close-exit-23' >&2; ",
        "printf 'fixture-stdout'; exit 23"
    );
    let mut request = ExecRequest::new(
        "sh",
        [
            "-c",
            script,
            "fixture",
            pid_path.to_str().unwrap(),
            ready_path.to_str().unwrap(),
        ],
    )
    .timeout(Duration::from_secs(2));
    request.stdin = Some(vec![b'x'; 128 * 1024]);

    let started = Instant::now();
    let raw_outcome = tokio::time::timeout(
        Duration::from_secs(5),
        run_once_with_gate_async(&request, wait_for_fixture_ready(ready_path)),
    )
    .await;
    let custody = finalize_fixture(&mut dir, &pid_path).await;
    let outcome = raw_outcome.unwrap_or_else(|elapsed| {
        panic!(
            "fixture execution exceeded its outer bound ({elapsed}); custody={custody:?}; evidence={}",
            dir.path().display()
        )
    });
    let error = match outcome {
        Err(error) => error,
        Ok(result) => panic!(
            "incomplete stdin unexpectedly returned a result {result:?}; custody={custody:?}; evidence={}",
            dir.path().display()
        ),
    };
    let delivery = observed_delivery_error(&error, &dir, &custody);
    assert_broken_pipe(delivery);
    assert_eq!(delivery.operation(), StdinDeliveryOperation::Write);
    let child = delivery.result();
    assert_eq!(child.code, Some(23));
    assert!(!child.success);
    assert!(!child.timed_out);
    assert_eq!(child.stdout, b"fixture-stdout");
    assert_eq!(child.stderr, b"stdin-close-exit-23");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_fixture_absent(&dir, &custody);
}

#[cfg(unix)]
#[tokio::test]
async fn successful_exit_after_partial_stdin_is_err_and_is_not_retried() {
    let mut dir = FixtureDir::new();
    let (pid_path, ready_path) = fixture_paths(&dir);
    let count_path = dir.path().join("attempts");
    let script = concat!(
        "printf x >> \"$1\"; ",
        "printf '%s' $$ > \"$2\"; ",
        "exec 0<&-; : > \"$3\"; ",
        "printf '{\"ok\":true,\"value\":{}}'; exit 0"
    );
    let mut request = ExecRequest::new(
        "sh",
        [
            "-c",
            script,
            "fixture",
            count_path.to_str().unwrap(),
            pid_path.to_str().unwrap(),
            ready_path.to_str().unwrap(),
        ],
    )
    .timeout(Duration::from_secs(2))
    .retry(RetryPolicy {
        max_retries: 2,
        delay: Duration::from_millis(1),
    });
    request.stdin = Some(vec![b'x'; 1024 * 1024]);

    let raw_outcome = tokio::time::timeout(Duration::from_secs(5), exec_async(&request)).await;
    let custody = finalize_fixture(&mut dir, &pid_path).await;
    let outcome = raw_outcome.unwrap_or_else(|elapsed| {
        panic!(
            "retry control exceeded its outer bound ({elapsed}); custody={custody:?}; evidence={}",
            dir.path().display()
        )
    });
    let error = match outcome {
        Err(error) => error,
        Ok(result) => panic!(
            "exit 0 after partial input unexpectedly returned {result:?}; custody={custody:?}; evidence={}",
            dir.path().display()
        ),
    };
    let delivery = observed_delivery_error(&error, &dir, &custody);
    assert_broken_pipe(delivery);
    assert_eq!(delivery.result().code, Some(0));
    assert!(delivery.result().success);
    assert!(!delivery.result().timed_out);
    assert_eq!(delivery.result().stdout, br#"{"ok":true,"value":{}}"#);
    assert_eq!(std::fs::read(&count_path).unwrap(), b"x");
    assert_fixture_absent(&dir, &custody);
}

#[cfg(unix)]
#[tokio::test]
async fn deferred_stdin_broken_pipe_keeps_the_original_deadline() {
    let mut dir = FixtureDir::new();
    let (pid_path, ready_path) = fixture_paths(&dir);
    let script = concat!(
        "printf '%s' $$ > \"$1\"; ",
        "exec 0<&-; : > \"$2\"; ",
        "printf 'hang-after-stdin-close' >&2; exec sleep 30"
    );
    let mut request = ExecRequest::new(
        "sh",
        [
            "-c",
            script,
            "fixture",
            pid_path.to_str().unwrap(),
            ready_path.to_str().unwrap(),
        ],
    )
    .timeout(Duration::from_secs(2));
    request.stdin = Some(vec![b'x'; 128 * 1024]);

    let started = Instant::now();
    let raw_outcome = tokio::time::timeout(
        Duration::from_secs(5),
        run_once_with_gate_async(&request, wait_for_fixture_ready(ready_path)),
    )
    .await;
    let custody = finalize_fixture(&mut dir, &pid_path).await;
    let outcome = raw_outcome.unwrap_or_else(|elapsed| {
        panic!(
            "hung child exceeded the outer bound ({elapsed}); custody={custody:?}; evidence={}",
            dir.path().display()
        )
    });
    let outcome = outcome.unwrap_or_else(|error| {
        panic!(
            "hung child returned an unexpected process error ({error:#}); custody={custody:?}; evidence={}",
            dir.path().display()
        )
    });
    assert!(outcome.timed_out, "{outcome:?}");
    assert!(!outcome.success);
    assert_eq!(outcome.code, None);
    assert!(outcome.stdout.is_empty());
    assert!(outcome.stderr.is_empty());
    assert!(started.elapsed() >= Duration::from_secs(1));
    assert_fixture_absent(&dir, &custody);
}

#[cfg(unix)]
#[tokio::test]
async fn capture_limit_failure_wins_over_deferred_stdin_broken_pipe() {
    for (name, output) in [("stdout", "printf 12345"), ("stderr", "printf 12345 >&2")] {
        let mut dir = FixtureDir::new();
        let (pid_path, ready_path) = fixture_paths(&dir);
        let script =
            format!("printf '%s' $$ > \"$1\"; exec 0<&-; : > \"$2\"; {output}; exec sleep 30");
        let mut request = ExecRequest::new(
            "sh",
            [
                "-c",
                &script,
                "fixture",
                pid_path.to_str().unwrap(),
                ready_path.to_str().unwrap(),
            ],
        )
        .timeout(Duration::from_secs(3))
        .output_limits(4, 4);
        request.stdin = Some(vec![b'x'; 128 * 1024]);

        let started = Instant::now();
        let raw_outcome = tokio::time::timeout(
            Duration::from_secs(5),
            run_once_with_gate_async(&request, wait_for_fixture_ready(ready_path)),
        )
        .await;
        let custody = finalize_fixture(&mut dir, &pid_path).await;
        let outcome = raw_outcome.unwrap_or_else(|elapsed| {
            panic!(
                "overflow child exceeded its outer bound ({elapsed}); custody={custody:?}; evidence={}",
                dir.path().display()
            )
        });
        let error = match outcome {
            Err(error) => error,
            Ok(result) => panic!(
                "capture overflow unexpectedly returned {result:?}; custody={custody:?}; evidence={}",
                dir.path().display()
            ),
        };
        assert!(
            error
                .to_string()
                .contains(&format!("captured {name} exceeded byte limit 4")),
            "{error:#}"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_fixture_absent(&dir, &custody);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn no_timeout_broken_pipe_still_kills_the_child_immediately() {
    let mut dir = FixtureDir::new();
    let (pid_path, ready_path) = fixture_paths(&dir);
    let script = "printf '%s' $$ > \"$1\"; exec 0<&-; : > \"$2\"; exec sleep 30";
    let mut request = ExecRequest::new(
        "sh",
        [
            "-c",
            script,
            "fixture",
            pid_path.to_str().unwrap(),
            ready_path.to_str().unwrap(),
        ],
    )
    .no_timeout();
    request.stdin = Some(vec![b'x'; 128 * 1024]);

    let started = Instant::now();
    let raw_outcome = tokio::time::timeout(
        Duration::from_secs(5),
        run_once_with_gate_async(&request, wait_for_fixture_ready(ready_path)),
    )
    .await;
    let custody = finalize_fixture(&mut dir, &pid_path).await;
    let outcome = raw_outcome.unwrap_or_else(|elapsed| {
        panic!(
            "no-timeout fixture exceeded the outer bound ({elapsed}); custody={custody:?}; evidence={}",
            dir.path().display()
        )
    });
    let error = match outcome {
        Err(error) => error,
        Ok(result) => panic!(
            "no-timeout BrokenPipe unexpectedly returned {result:?}; custody={custody:?}; evidence={}",
            dir.path().display()
        ),
    };
    assert!(
        error.to_string().contains("writing stdin to child"),
        "{error:#}"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_fixture_absent(&dir, &custody);
}

#[cfg(unix)]
#[test]
fn sync_facade_preserves_child_diagnostics_after_stdin_broken_pipe() {
    let mut dir = FixtureDir::new();
    let (pid_path, ready_path) = fixture_paths(&dir);
    let script = "printf '%s' $$ > \"$1\"; exec 0<&-; : > \"$2\"; printf 'sync-stdin-close-marker' >&2; exit 23";
    let mut request = ExecRequest::new(
        "sh",
        [
            "-c",
            script,
            "fixture",
            pid_path.to_str().unwrap(),
            ready_path.to_str().unwrap(),
        ],
    )
    .timeout(Duration::from_secs(2));
    request.stdin = Some(vec![b'x'; 2 * 1024 * 1024]);

    let raw_outcome = exec_sync(&request);
    let custody = finalize_fixture_sync(&mut dir, &pid_path);
    let error = match raw_outcome {
        Err(error) => error,
        Ok(result) => panic!(
            "partial synchronous input unexpectedly returned {result:?}; custody={custody:?}; evidence={}",
            dir.path().display()
        ),
    };
    let delivery = observed_delivery_error(&error, &dir, &custody);
    assert_broken_pipe(delivery);
    assert_eq!(delivery.result().code, Some(23));
    assert_eq!(delivery.result().stderr, b"sync-stdin-close-marker");
    assert_fixture_absent(&dir, &custody);
}

#[cfg(all(target_os = "macos", feature = "test-support"))]
#[test]
fn darwin_async_spawn_waits_for_registry_lock_before_launch() {
    use std::sync::mpsc;
    use std::thread;

    let directory = tempfile::tempdir().unwrap();
    let launched_path = directory.path().join("launched");
    let request = ExecRequest::new(
        "/bin/sh",
        [
            "-c",
            "printf launched > \"$1\"; exit 0",
            "fixture",
            launched_path.to_str().unwrap(),
        ],
    );
    let before_contention = registry_contention_count();
    let (started_sender, started_receiver) = mpsc::sync_channel(1);

    let worker = coordinate_children(|_| {
        let worker = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            started_sender
                .send(())
                .expect("worker readiness receiver is live");
            runtime.block_on(async {
                let mut command = tokio::process::Command::new(&request.program);
                configure_async_command(&mut command, &request);
                command.kill_on_drop(true);
                let (child, registration) =
                    spawn_and_register_async(&mut command, &request).unwrap();
                let pid = child.id().unwrap();
                let mut owned = OwnedChild::with_registration(child, registration);
                let success =
                    match tokio::time::timeout(Duration::from_secs(2), owned.child.wait()).await {
                        Ok(Ok(status)) => {
                            owned.disarm();
                            status.success()
                        }
                        Ok(Err(_)) => false,
                        Err(_) => {
                            let _cleanup = owned.kill_and_reap().await;
                            false
                        }
                    };
                (pid, success)
            })
        });

        let worker_started = started_receiver
            .recv_timeout(Duration::from_secs(2))
            .is_ok();
        let contention_deadline = Instant::now() + Duration::from_secs(2);
        while registry_contention_count() <= before_contention
            && worker_started
            && Instant::now() < contention_deadline
        {
            thread::yield_now();
        }
        let contended = registry_contention_count() > before_contention;

        let launch_deadline = Instant::now() + Duration::from_millis(100);
        while !launched_path.exists() && Instant::now() < launch_deadline {
            thread::yield_now();
        }
        let blocked_before_unlock = !launched_path.exists();
        (worker, worker_started, contended, blocked_before_unlock)
    });

    let (worker, worker_started, contended, blocked_before_unlock) = worker;
    let joined = worker.join();
    let worker_panicked = joined.is_err();
    let (pid, success) = joined.unwrap_or((0, false));
    assert!(
        worker_started,
        "spawn worker reached the coordinated operation"
    );
    assert!(
        contended,
        "spawn worker contended on the held child registry"
    );
    assert!(
        blocked_before_unlock,
        "child launch must wait until its registry transaction owns the lock"
    );
    assert!(!worker_panicked, "spawn worker completed without panic");
    assert!(success, "registered child {pid} must exit successfully");
    assert!(launched_path.is_file(), "child launches after lock release");
    assert!(
        !coordinate_children(|registry| registry.contains(pid)),
        "successful wait releases the exact PID reservation"
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn darwin_owned_wait_retains_registration_while_pending_and_releases_after_success() {
    use std::os::unix::process::CommandExt;

    struct ReleaseGate {
        path: PathBuf,
        released: bool,
    }

    impl ReleaseGate {
        fn release(&mut self) -> io::Result<()> {
            let result = std::fs::write(&self.path, b"release");
            self.released = result.is_ok();
            result
        }
    }

    impl Drop for ReleaseGate {
        fn drop(&mut self) {
            if !self.released {
                drop(std::fs::write(&self.path, b"release"));
            }
        }
    }

    let directory = tempfile::tempdir().unwrap();
    let ready_path = directory.path().join("ready.pid");
    let release_path = directory.path().join("release");
    let mut gate = ReleaseGate {
        path: release_path.clone(),
        released: false,
    };
    let script = concat!(
        "printf '%s' \"$$\" > \"$1\"; ",
        "n=0; while [ ! -f \"$2\" ] && [ \"$n\" -lt 200 ]; do ",
        "n=$((n + 1)); sleep 0.01; done; ",
        "[ -f \"$2\" ] || exit 99; exit 0"
    );
    let request = ExecRequest::new(
        "/bin/sh",
        [
            "-c",
            script,
            "fixture",
            ready_path.to_str().unwrap(),
            release_path.to_str().unwrap(),
        ],
    );
    let mut command = tokio::process::Command::new(&request.program);
    configure_async_command(&mut command, &request);
    command.kill_on_drop(true);
    command.as_std_mut().process_group(0);
    let (child, registration) = spawn_and_register_async(&mut command, &request).unwrap();
    let pid = child.id().unwrap();
    let mut owned = OwnedChild::with_registration(child, registration);

    let ready_deadline = Instant::now() + Duration::from_secs(1);
    while !ready_path.is_file() && Instant::now() < ready_deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let ready_pid = std::fs::read_to_string(&ready_path)
        .ok()
        .and_then(|text| text.parse::<u32>().ok());
    let ready_for_owned_child = ready_pid == Some(pid);
    let registered_before_wait = coordinate_children(|registry| registry.contains(pid));
    let first_wait_pending = tokio::time::timeout(Duration::from_millis(10), owned.child.wait())
        .await
        .is_err();
    let registered_while_pending = coordinate_children(|registry| registry.contains(pid));

    let release_result = gate.release();
    let final_wait = tokio::time::timeout(Duration::from_secs(2), owned.child.wait()).await;
    let final_success = matches!(&final_wait, Ok(Ok(status)) if status.success());
    let registered_after_wait = coordinate_children(|registry| registry.contains(pid));
    if final_success {
        owned.disarm();
    }
    drop(owned);
    let released_after_successful_disarm = !coordinate_children(|registry| registry.contains(pid));

    assert!(release_result.is_ok(), "release the exact child gate");
    assert!(
        ready_for_owned_child,
        "child readiness records the owned PID"
    );
    assert!(
        registered_before_wait,
        "registration exists before direct wait"
    );
    assert!(
        first_wait_pending,
        "the withheld child leaves direct wait Pending"
    );
    assert!(
        registered_while_pending,
        "Pending direct wait retains registration"
    );
    assert!(
        final_success,
        "release lets the direct child wait succeed: {final_wait:?}"
    );
    assert!(
        registered_after_wait,
        "owner retains registration through its wait"
    );
    assert!(
        released_after_successful_disarm,
        "successful direct wait disarms the PID reservation"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn reaped_cleanup_error_disarms_owned_state_before_return() {
    use std::os::unix::process::CommandExt;

    let request = ExecRequest::new("/bin/sh", ["-c", "exit 0"]);
    let mut command = tokio::process::Command::new(&request.program);
    configure_async_command(&mut command, &request);
    command.kill_on_drop(true);
    command.as_std_mut().process_group(0);
    #[cfg(target_os = "macos")]
    let (child, registration) = spawn_and_register_async(&mut command, &request).unwrap();
    #[cfg(not(target_os = "macos"))]
    let child = command.spawn().unwrap();
    #[cfg(target_os = "macos")]
    let pid = child.id().unwrap();
    #[cfg(target_os = "macos")]
    let mut owned = OwnedChild::with_registration(child, registration);
    #[cfg(not(target_os = "macos"))]
    let mut owned = OwnedChild::new(child);

    #[cfg(target_os = "macos")]
    let registered_before_reap = coordinate_children(|registry| registry.contains(pid));
    let wait_and_cleanup = owned
        .reap_and_finish(Err(anyhow::anyhow!("injected group-cleanup error")), Ok(()))
        .await
        .expect("the production helper completed a native child wait");
    let (status, cleanup_result) = wait_and_cleanup;
    let waited_successfully = status.success();
    let error = cleanup_result.unwrap_err();
    let retained_cleanup_error = error.to_string().contains("injected group-cleanup error");
    let group_cleared = owned.group.is_none();
    #[cfg(target_os = "macos")]
    let released_registration = !coordinate_children(|registry| registry.contains(pid));
    #[cfg(not(target_os = "macos"))]
    let released_registration = true;
    drop(owned);

    assert!(
        waited_successfully,
        "the native success fixture child exited successfully"
    );
    assert!(
        retained_cleanup_error,
        "the original cleanup error is preserved"
    );
    assert!(group_cleared, "the reaped process group is disarmed");
    #[cfg(target_os = "macos")]
    assert!(
        registered_before_reap,
        "native child is registered before its wait"
    );
    assert!(
        released_registration,
        "a reaped PID and group are disarmed before cleanup errors propagate"
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn darwin_cancellation_releases_wait_status_after_child_cleanup() {
    let mut directory = FixtureDir::new();
    let pid_path = directory.path().join("cancelled-child.pid");
    let script = "printf '%s' $$ > \"$1\"; exec /bin/sleep 30";
    let mut request = ExecRequest::new(
        "/bin/sh",
        ["-c", script, "fixture", pid_path.to_str().unwrap()],
    );
    request.stdin = Some(vec![b'x'; 4096]);
    let run =
        tokio::spawn(
            async move { run_once_with_gate_async(&request, std::future::pending()).await },
        );

    let ready = tokio::time::timeout(
        Duration::from_secs(1),
        wait_for_fixture_ready(pid_path.clone()),
    )
    .await;
    let ready_observed = matches!(ready.as_ref(), Ok(Ok(())));
    let pid = std::fs::read_to_string(&pid_path)
        .ok()
        .and_then(|text| text.parse::<u32>().ok());
    let registered_while_running =
        pid.is_some_and(|pid| coordinate_children(|registry| registry.contains(pid)));

    run.abort();
    let cancelled = run.await.is_err_and(|error| error.is_cancelled());
    let custody = finalize_fixture(&mut directory, &pid_path).await;
    let released_after_cancellation =
        pid.is_some_and(|pid| !coordinate_children(|registry| registry.contains(pid)));

    assert!(ready_observed, "child must reach the gated stdin phase");
    assert!(
        registered_while_running,
        "child wait ownership remains live until cancellation"
    );
    assert!(
        cancelled,
        "the execution future must observe task cancellation"
    );
    assert_fixture_absent(&directory, &custody);
    assert!(
        released_after_cancellation,
        "cancellation releases the child reservation"
    );
}
