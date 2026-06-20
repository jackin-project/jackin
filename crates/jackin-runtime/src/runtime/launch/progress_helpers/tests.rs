use super::StepCounter;
use crate::runtime::progress::LaunchProgress;
use jackin_launch::{LaunchCancelled, LaunchDiagnostics};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;

struct TestDiagnostics;

impl LaunchDiagnostics for TestDiagnostics {
    fn run_id(&self) -> &'static str {
        "test-run"
    }
    fn path(&self) -> &'static Path {
        Path::new("/tmp/jackin-test-run.jsonl")
    }
    fn command_output_path(&self, name: &str) -> PathBuf {
        PathBuf::from(format!("/tmp/jackin-test-{name}.log"))
    }
    fn compact(&self, _kind: &str, _message: &str) {}
    fn stage(&self, _kind: &str, _stage: &str, _message: &str, _detail: Option<&str>) {}
}

/// A `StepCounter` carrying a rich-surface token, optionally pre-cancelled
/// to stand in for an operator who has already hit Ctrl+C.
fn steps_with_progress(cancelled: bool) -> StepCounter {
    let progress = LaunchProgress::for_test(Arc::new(TestDiagnostics));
    if cancelled {
        progress.cancel_token().cancel();
    }
    let mut steps = StepCounter::new("test-role");
    steps.start_progress(progress);
    steps
}

#[tokio::test]
async fn next_bails_at_checkpoint_when_cancelled() {
    let mut steps = steps_with_progress(true);
    let err = steps
        .next("Launching role")
        .await
        .expect_err("a cancelled token must abort at the step boundary");
    assert!(
        LaunchCancelled::is_cancel(&err),
        "cancel must carry the typed sentinel, not a generic error: {err}"
    );
}

#[tokio::test]
async fn next_proceeds_when_not_cancelled() {
    let mut steps = steps_with_progress(false);
    steps
        .next("Launching role")
        .await
        .expect("an un-cancelled step boundary must proceed");
}

#[tokio::test]
async fn run_blocking_aborts_while_worker_still_blocked() {
    let steps = steps_with_progress(true);
    // The worker parks on `rx.recv()` and never returns on its own; if
    // `run_blocking` waited for the join it would hang forever. A prompt
    // cancel error proves the token race wins over the in-flight worker.
    let (_tx, rx) = mpsc::channel::<()>();
    let result: anyhow::Result<()> = steps
        .run_blocking(move || {
            let _ = rx.recv();
            anyhow::Ok(())
        })
        .await;
    let err = result.expect_err("a cancelled token must abort the blocking join");
    assert!(LaunchCancelled::is_cancel(&err), "got: {err}");
    // `_tx` drops here, unblocking the orphaned worker so it exits cleanly.
}

#[tokio::test]
async fn run_blocking_returns_value_when_not_cancelled() {
    let steps = steps_with_progress(false);
    let value = steps
        .run_blocking(|| anyhow::Ok(42u32))
        .await
        .expect("an un-cancelled blocking op must pass its value through");
    assert_eq!(value, 42);
}

#[tokio::test]
async fn run_blocking_runs_in_headless_path_without_token() {
    // No progress surface (headless launch): there is no cancel token, so
    // `run_blocking` simply awaits the worker.
    let steps = StepCounter::new("test-role");
    let value = steps
        .run_blocking(|| anyhow::Ok(7u32))
        .await
        .expect("headless blocking op must run");
    assert_eq!(value, 7);
}

#[tokio::test]
async fn while_waiting_aborts_pending_future_when_cancelled() {
    let steps = steps_with_progress(true);
    let result: anyhow::Result<()> = steps.while_waiting(std::future::pending()).await;
    let err = result.expect_err("a cancelled token must abort a pending await");
    assert!(LaunchCancelled::is_cancel(&err), "got: {err}");
}
