// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::{LaunchProgress, failure_acknowledged};
use crate::LaunchDiagnostics;
use crate::tui::components::progress_rail::{
    LABEL_SLIDE_FRAMES, animated_label_center, display_stage_statuses, label_strip, labels_line,
};
use crate::{
    LaunchFailure, LaunchStage, StageStatus, active_stage_index, initial_view, update_stage,
};
use jackin_diagnostics::RunDiagnostics;

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
    fn stage(&self, _kind: &str, _stage: LaunchStage, _message: &str, _detail: Option<&str>) {}
}

fn test_progress() -> LaunchProgress {
    LaunchProgress::for_test(Arc::new(TestDiagnostics))
}

fn test_diagnostics() -> Arc<RunDiagnostics> {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    RunDiagnostics::start(&paths, false, "load").unwrap()
}

fn dummy_failure() -> LaunchFailure {
    LaunchFailure {
        title: "boom".to_owned(),
        summary: "it failed".to_owned(),
        detail: None,
        next_step: None,
        stage: LaunchStage::Network,
        diagnostics_path: None,
        command_output_path: None,
    }
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
#[tokio::test]
async fn stage_failed_does_not_block_on_test_renderer() {
    // The Rich path waits for an operator Enter/Esc dismiss. The test
    // renderer returns immediately so failure-state tests do not hang.
    let mut progress = LaunchProgress::for_test(test_diagnostics());
    tokio::time::timeout(
        Duration::from_millis(500),
        progress.stage_failed(dummy_failure()),
    )
    .await
    .expect("stage_failed must not block on the test renderer");
    assert!(progress.view_for_test().lock().unwrap().failure.is_some());
    assert!(!progress.view_for_test().lock().unwrap().failure_ack);
}
#[tokio::test]
async fn stage_failed_writes_full_detail_to_diagnostics() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, false, "load").unwrap();
    let diagnostics: Arc<RunDiagnostics> = Arc::clone(&run);
    let mut progress = LaunchProgress::for_test(diagnostics);

    progress
            .stage_failed(LaunchFailure {
                title: "Launch failed".to_owned(),
                summary: "preparing kimi binary".to_owned(),
                detail: Some(
                    "preparing kimi binary: resolving latest kimi binary: https://code.kimi.com/kimi-code/latest failed: curl: (28) Connection timed out after 30001 milliseconds".to_owned(),
                ),
                next_step: None,
                stage: LaunchStage::DerivedImage,
                diagnostics_path: None,
                command_output_path: None,
            })
            .await;

    let body = std::fs::read_to_string(run.path()).unwrap();
    // Schema v2 may label the event as `event.name` rather than `kind`.
    assert!(
        body.contains("stage_failed") || body.contains("launch_failed"),
        "expected failure diagnostic: {body}"
    );
    assert!(
        body.contains("preparing kimi binary"),
        "expected summary in diagnostics: {body}"
    );
    assert!(
        body.contains("Connection timed out after 30001 milliseconds"),
        "expected full detail in diagnostics: {body}"
    );
}
#[tokio::test]
async fn stage_failed_resets_prior_ack() {
    // A second failure must start un-acked: a stale ack left over from a
    // previously dismissed popup would otherwise auto-dismiss the new one.
    let mut progress = LaunchProgress::for_test(test_diagnostics());
    progress.stage_failed(dummy_failure()).await;
    progress.view_for_test().lock().unwrap().failure_ack = true;
    progress.stage_failed(dummy_failure()).await;
    assert!(!progress.view_for_test().lock().unwrap().failure_ack);
}
#[test]
fn select_choice_errors_without_rich_renderer() {
    let mut progress = LaunchProgress::for_test(test_diagnostics());
    let error = progress
        .select_choice("pick", vec!["a".into(), "b".into()])
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("requires the rich launch dialog")
    );
}
#[test]
fn env_prompts_error_without_rich_renderer() {
    let mut progress = LaunchProgress::for_test(test_diagnostics());

    assert!(
        progress
            .prompt_text("API key", None, true)
            .unwrap_err()
            .to_string()
            .contains("requires the rich launch dialog")
    );
    assert!(
        progress
            .prompt_select("Project", &["web".to_owned()], None, false)
            .unwrap_err()
            .to_string()
            .contains("requires the rich launch dialog")
    );
}
#[tokio::test]
async fn test_renderer_does_not_delay_stage_settle() {
    let progress = LaunchProgress::for_test(test_diagnostics());
    tokio::time::timeout(Duration::from_millis(20), progress.settle_stage_visual())
        .await
        .expect("test renderer should not sleep");
}
#[test]
fn update_stage_sets_one_rows_status_and_detail() {
    let mut view = initial_view();
    update_stage(&mut view, LaunchStage::Network, StageStatus::Done, "up");
    let net = view
        .stages
        .iter()
        .find(|r| r.stage == LaunchStage::Network)
        .unwrap();
    assert_eq!(net.status, StageStatus::Done);
    assert_eq!(net.detail, "up");
    // A different stage is left untouched.
    let cap = view
        .stages
        .iter()
        .find(|r| r.stage == LaunchStage::Capsule)
        .unwrap();
    assert_ne!(cap.status, StageStatus::Done);
}
#[test]
fn stage_labels_are_stable() {
    let labels: Vec<&str> = LaunchStage::ALL.iter().map(|stage| stage.label()).collect();
    assert_eq!(
        labels,
        vec![
            "identity",
            "role",
            "credentials",
            "construct",
            "agent binaries",
            "derived image",
            "workspace",
            "network",
            "sidecar",
            "capsule",
            "hardline"
        ]
    );
}
#[test]
fn failed_stage_is_the_active_progress_label() {
    let mut view = initial_view();
    update_stage(
        &mut view,
        LaunchStage::Credentials,
        StageStatus::Done,
        "ready",
    );
    update_stage(
        &mut view,
        LaunchStage::Construct,
        StageStatus::Done,
        "ready",
    );
    update_stage(
        &mut view,
        LaunchStage::DerivedImage,
        StageStatus::Failed,
        "Building the Docker container failed.",
    );

    assert_eq!(
        view.stages[active_stage_index(&view)].stage,
        LaunchStage::DerivedImage
    );
    let labels = labels_line(&view, true, 80);
    let failed = labels
        .spans
        .iter()
        .find(|span| span.content == "derived image")
        .expect("failed stage label should be visible");
    assert_eq!(
        failed.style.fg,
        Some(
            termrock::Theme::default()
                .style(termrock::style::Role::Danger)
                .fg
                .unwrap_or_default()
        )
    );
}
#[test]
fn progress_display_masks_out_of_order_completed_stages() {
    let mut view = initial_view();
    update_stage(&mut view, LaunchStage::Identity, StageStatus::Done, "ready");
    update_stage(
        &mut view,
        LaunchStage::Role,
        StageStatus::Running,
        "resolving role",
    );
    update_stage(
        &mut view,
        LaunchStage::Workspace,
        StageStatus::Done,
        "materialized early",
    );

    let statuses = display_stage_statuses(&view);
    assert_eq!(statuses[0], StageStatus::Done);
    assert_eq!(statuses[1], StageStatus::Running);
    assert!(
        statuses[2..]
            .iter()
            .all(|status| *status == StageStatus::Queued),
        "later out-of-order completions must not punch green holes in the progress rail: {statuses:?}"
    );
}
#[test]
fn progress_display_fills_every_prior_stage_sequentially() {
    let mut view = initial_view();
    update_stage(
        &mut view,
        LaunchStage::Identity,
        StageStatus::Skipped,
        "already known",
    );
    update_stage(&mut view, LaunchStage::Role, StageStatus::Done, "trusted");
    update_stage(
        &mut view,
        LaunchStage::Credentials,
        StageStatus::Done,
        "resolved",
    );
    update_stage(
        &mut view,
        LaunchStage::Construct,
        StageStatus::Done,
        "online",
    );
    update_stage(
        &mut view,
        LaunchStage::AgentBinaries,
        StageStatus::Done,
        "cached",
    );
    update_stage(
        &mut view,
        LaunchStage::DerivedImage,
        StageStatus::Running,
        "building",
    );

    let statuses = display_stage_statuses(&view);
    assert_eq!(
        &statuses[..6],
        &[
            StageStatus::Done,
            StageStatus::Done,
            StageStatus::Done,
            StageStatus::Done,
            StageStatus::Done,
            StageStatus::Running,
        ]
    );
}
#[test]
fn active_stage_uses_the_sequential_frontier() {
    let mut view = initial_view();
    update_stage(&mut view, LaunchStage::Identity, StageStatus::Done, "ready");
    update_stage(
        &mut view,
        LaunchStage::Workspace,
        StageStatus::Running,
        "polling workspace",
    );

    assert_eq!(
        view.stages[active_stage_index(&view)].stage,
        LaunchStage::Identity
    );
}
#[test]
fn stage_label_transition_slides_between_centers() {
    let mut view = initial_view();
    update_stage(&mut view, LaunchStage::Identity, StageStatus::Done, "ready");
    update_stage(
        &mut view,
        LaunchStage::Role,
        StageStatus::Running,
        "resolving role",
    );

    let transition = view
        .label_transition
        .expect("active stage change should start a label slide");
    assert_eq!(transition.from, 0);
    assert_eq!(transition.to, 1);

    view.frame = transition.start_frame + LABEL_SLIDE_FRAMES / 2;
    let active = active_stage_index(&view);
    let display_statuses = display_stage_statuses(&view);
    let (_, centers) = label_strip(&view, active, false, &display_statuses);
    let center = animated_label_center(&view, &centers).unwrap();
    assert!(center > centers[0], "label viewport should move right");
    assert!(
        center < centers[1],
        "label viewport should not snap to the target"
    );
}
