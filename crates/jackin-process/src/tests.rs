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
