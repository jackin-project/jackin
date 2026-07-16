// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{StepCounter, stage_index, telemetry_stage};
use crate::runtime::progress::LaunchProgress;
use jackin_launch_tui::{LaunchCancelled, LaunchDiagnostics};
use std::sync::Arc;

struct TestDiagnostics;

impl LaunchDiagnostics for TestDiagnostics {
    fn run_id(&self) -> &'static str {
        "test-run"
    }
    fn compact(&self, _kind: &str, _message: &str) {}
    fn error(&self, _kind: &str, _message: &str, _error_type: Option<&str>) {}
    fn stage(
        &self,
        _kind: &str,
        _stage: jackin_core::LaunchStage,
        _message: &str,
        _detail: Option<&str>,
    ) {
    }
}

/// A `StepCounter` carrying a rich-surface token, optionally pre-cancelled
/// to stand in for an operator who has already hit Ctrl+C.
fn steps_with_progress(cancelled: bool) -> StepCounter {
    let progress = LaunchProgress::for_test(Arc::new(TestDiagnostics));
    if cancelled {
        progress.cancel_token().cancel();
    }
    let mut steps = StepCounter::new(
        "test-role",
        jackin_telemetry::schema::enums::LaunchTargetKind::Directory,
    );
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
    steps.stage_done(jackin_core::LaunchStage::Capsule, "ready");
}

#[test]
fn launch_stage_registry_mapping_is_exhaustive_and_ordered() {
    for (index, stage) in jackin_core::LaunchStage::ALL.into_iter().enumerate() {
        assert_eq!(stage_index(stage), index);
        assert_eq!(
            telemetry_stage(stage),
            jackin_telemetry::schema::enums::LaunchStageName::ALL[index]
        );
    }
}

#[test]
fn renderer_teardown_does_not_end_hardline_telemetry() {
    let mut steps = steps_with_progress(false);
    steps.opening_hardline();
    let index = stage_index(jackin_core::LaunchStage::Hardline);
    assert!(steps.stage_telemetry[index].is_some());

    steps.finish_progress();
    assert!(steps.stage_telemetry[index].is_some());

    steps.stage_done(jackin_core::LaunchStage::Hardline, "open");
    assert!(steps.stage_telemetry[index].is_none());
}
