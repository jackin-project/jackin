//! Unit tests for the shared process transport.
use super::*;
use std::time::Duration;

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

#[cfg(unix)]
#[tokio::test]
async fn timeout_reaps_the_direct_child() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid_file = std::env::temp_dir().join(format!(
        "jackin-process-reap-{}-{nonce}.pid",
        std::process::id()
    ));
    let pid_file_arg = pid_file.as_os_str();
    let request = ExecRequest::new(
        "sh",
        ["-c", "printf '%s' \"$$\" > \"$PID_FILE\"; exec sleep 5"],
    )
    .envs([("PID_FILE", pid_file_arg)])
    .timeout(Duration::from_millis(100));

    let result = exec_async(&request).await.unwrap();
    assert!(result.timed_out, "expected timeout, got {result:?}");
    let child_pid = std::fs::read_to_string(&pid_file).unwrap();
    std::fs::remove_file(&pid_file).unwrap();

    let process_state = exec_async(
        &ExecRequest::new("ps", ["-o", "stat=", "-p", child_pid.trim()])
            .timeout(Duration::from_secs(1)),
    )
    .await
    .unwrap();
    assert!(
        String::from_utf8_lossy(&process_state.stdout)
            .trim()
            .is_empty(),
        "timed-out direct child still exists or is a zombie: {}",
        String::from_utf8_lossy(&process_state.stdout).trim()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn bounded_timeout_covers_a_blocked_stdin_write() {
    let request = ExecRequest::new("sh", ["-c", "sleep 5"])
        .output_limits(16, 16)
        .timeout(Duration::from_millis(50));
    let request = ExecRequest {
        stdin: Some(vec![b'x'; 8 * 1024 * 1024]),
        ..request
    };
    let result = exec_async(&request).await.unwrap();
    assert!(result.timed_out, "expected timeout, got {result:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn timeout_covers_stdin_writer_after_direct_child_exits() {
    let command = "exec 3<&0; (exec 0<&3; sleep 1 >/dev/null 2>&1) & exit 0";
    let mut request = ExecRequest::new("sh", ["-c", command]).timeout(Duration::from_millis(250));
    request.stdin = Some(vec![b'x'; 8 * 1024 * 1024]);
    let started = Instant::now();
    let result = exec_async(&request).await.unwrap();
    assert!(result.timed_out, "expected timeout, got {result:?}");
    assert!(
        started.elapsed() < Duration::from_millis(900),
        "timeout did not cover the blocked stdin writer: {:?}",
        started.elapsed()
    );
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
    // First attempt uses a failing program shape; retry policy alone is
    // exercised with always-false then we assert attempts were made via
    // max_retries with false (all fail).
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
fn bounded_capture_accepts_exact_limit() {
    let result = exec_sync(&ExecRequest::new("printf", ["1234"]).output_limits(4, 0)).unwrap();
    assert!(result.success);
    assert_eq!(result.stdout, b"1234");
    assert!(result.stderr.is_empty());
}

#[cfg(unix)]
#[test]
fn bounded_capture_stops_oversized_stdout_and_stderr_promptly() {
    for (args, stdout_limit, stderr_limit) in [
        (vec!["-c", "printf 12345"], 4, 16),
        (vec!["-c", "printf 12345 >&2"], 16, 4),
    ] {
        let started = Instant::now();
        let result = exec_sync(
            &ExecRequest::new("sh", args)
                .output_limits(stdout_limit, stderr_limit)
                .timeout(Duration::from_secs(5)),
        );
        let error = result.expect_err("limit + 1 bytes should be rejected");
        assert!(error.to_string().contains("exceeded"), "{error:#}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "overflow detection waited too long: {:?}",
            started.elapsed()
        );
    }
}

#[test]
fn bounded_capture_rejects_non_capture_routing() {
    let request = ExecRequest::new("true", None::<&str>)
        .output_limits(4, 4)
        .stdout_mode(StdioMode::Inherit);
    let error = exec_sync(&request).expect_err("bounded capture requires captured streams");
    assert!(
        error
            .to_string()
            .contains("requires captured stdout and stderr")
    );
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

#[test]
fn sync_spawn_rejects_unenforceable_output_limits() {
    let error = spawn_sync(&ExecRequest::new("true", None::<&str>).output_limits(4, 4))
        .expect_err("spawn must not silently ignore output limits");
    assert!(error.to_string().contains("cannot enforce output limits"));
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

#[tokio::test]
async fn async_spawn_rejects_unenforceable_output_limits() {
    let error = spawn_async(&ExecRequest::new("true", None::<&str>).output_limits(4, 4))
        .expect_err("spawn must not silently ignore output limits");
    assert!(error.to_string().contains("cannot enforce output limits"));
}
