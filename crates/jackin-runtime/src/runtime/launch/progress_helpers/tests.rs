use super::StepCounter;
use crate::runtime::progress::LaunchProgress;
use jackin_launch_tui::{LaunchCancelled, LaunchDiagnostics};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
    fn error(&self, _kind: &str, _message: &str, _error_type: Option<&str>) {}
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
