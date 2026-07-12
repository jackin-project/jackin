use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{LaunchProgress, failure_acknowledged};
use crate::LaunchDiagnostics;

struct TestDiagnostics;

impl LaunchDiagnostics for TestDiagnostics {
    fn run_id(&self) -> &'static str {
        "test-run"
    }
    fn path(&self) -> &Path {
        Path::new("/tmp")
    }
    fn persists(&self) -> bool {
        true
    }
    fn command_output_path(&self, name: &str) -> PathBuf {
        PathBuf::from("/tmp").join(name)
    }
    fn compact(&self, _kind: &str, _message: &str) {}
    fn error(&self, _kind: &str, _message: &str, _error_type: Option<&str>) {}
    fn stage(&self, _kind: &str, _stage: &str, _message: &str, _detail: Option<&str>) {}
}

fn test_progress() -> LaunchProgress {
    LaunchProgress::for_test(Arc::new(TestDiagnostics))
}

#[tokio::test]
async fn while_waiting_passes_through_ok_result() {
    let progress = test_progress();
    let result = progress.while_waiting(async { anyhow::Ok(42u32) }).await;
    assert_eq!(result.unwrap(), 42);
}

#[tokio::test]
async fn while_waiting_returns_cancel_error_when_token_fired() {
    let progress = test_progress();
    progress.cancel_token().cancel();
    let result: anyhow::Result<u32> = progress.while_waiting(std::future::pending()).await;
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("cancelled by operator"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn while_waiting_passes_through_inner_error() {
    let progress = test_progress();
    let result: anyhow::Result<u32> = progress
        .while_waiting(async { anyhow::bail!("inner failure") })
        .await;
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("inner failure"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn cancel_after_while_waiting_started_interrupts_pending_future() {
    let progress = test_progress();
    let token = progress.cancel_token();
    // Yield once so while_waiting starts polling before the cancel fires.
    tokio::spawn(async move {
        tokio::task::yield_now().await;
        token.cancel();
    });
    let result: anyhow::Result<u32> = progress.while_waiting(std::future::pending()).await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("cancelled by operator")
    );
}

#[test]
#[allow(clippy::panic, reason = "documented residual allow; prefer expect when site is lint-true")]
fn poisoned_failure_ack_lock_recovers_without_auto_acknowledging() {
    let progress = test_progress();
    let view = Arc::clone(progress.view_for_test());
    let poison_view = Arc::clone(&view);
    drop(
        std::thread::spawn(move || {
            let _guard = poison_view
                .lock()
                .expect("test view lock should be healthy");
            panic!("poison test view lock");
        })
        .join(),
    );

    assert!(
        !failure_acknowledged(&view),
        "poisoned lock must not be treated as acknowledged"
    );

    {
        let mut view = view
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        view.failure_ack = true;
    }

    assert!(failure_acknowledged(&view));
}
