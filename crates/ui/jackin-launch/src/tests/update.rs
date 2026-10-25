// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{LaunchFailure, LaunchIdentity, LaunchStage, LaunchTargetKind, StageStatus};

use crate::tui::message::LaunchMessage;
use crate::tui::update::{active_stage_index, initial_view, update_launch_view, update_stage};

fn identity(target_kind: LaunchTargetKind) -> LaunchIdentity {
    LaunchIdentity {
        role: "myrole".to_owned(),
        agent: "codex".to_owned(),
        target_kind,
        target_label: "workspace".to_owned(),
        mounts: Vec::new(),
        image: None,
        container: None,
    }
}

fn failure() -> LaunchFailure {
    LaunchFailure {
        title: "launch failed".to_owned(),
        summary: "container exited".to_owned(),
        detail: None,
        next_step: None,
        stage: LaunchStage::Capsule,
    }
}

#[test]
fn initial_view_starts_with_all_stages_queued() {
    let view = initial_view();
    assert_eq!(view.stages.len(), LaunchStage::ALL.len());
    assert!(
        view.stages
            .iter()
            .all(|row| row.status == StageStatus::Queued && row.detail == "queued")
    );
    assert_eq!(view.status, "preparing launch");
    assert!(view.failure.is_none());
    assert_eq!(view.frame, 0);
}

#[test]
fn started_message_records_identity_with_workspace_preposition() {
    let mut view = initial_view();
    drop(update_launch_view(
        &mut view,
        LaunchMessage::Started(identity(LaunchTargetKind::Workspace)),
    ));
    assert_eq!(
        view.identity.as_ref().map(|id| id.role.as_str()),
        Some("myrole")
    );
    assert_eq!(view.status, "loading myrole into workspace");
}

#[test]
fn started_message_uses_directory_preposition() {
    let mut view = initial_view();
    drop(update_launch_view(
        &mut view,
        LaunchMessage::Started(identity(LaunchTargetKind::Directory)),
    ));
    assert_eq!(view.status, "loading myrole in directory");
}

#[test]
fn stage_status_with_activity_replaces_view_status() {
    let mut view = initial_view();
    drop(update_launch_view(
        &mut view,
        LaunchMessage::StageStatus {
            stage: LaunchStage::Role,
            status: StageStatus::Running,
            detail: "resolving role".to_owned(),
            set_activity: true,
        },
    ));
    let row = view
        .stages
        .iter()
        .find(|row| row.stage == LaunchStage::Role)
        .expect("role row");
    assert_eq!(row.status, StageStatus::Running);
    assert_eq!(row.detail, "resolving role");
    assert_eq!(view.status, "resolving role");
}

#[test]
fn stage_status_without_activity_preserves_view_status() {
    let mut view = initial_view();
    drop(update_launch_view(
        &mut view,
        LaunchMessage::StageStatus {
            stage: LaunchStage::Role,
            status: StageStatus::Done,
            detail: "role ready".to_owned(),
            set_activity: false,
        },
    ));
    assert_eq!(view.status, "preparing launch");
    let row = view
        .stages
        .iter()
        .find(|row| row.stage == LaunchStage::Role)
        .expect("role row");
    assert_eq!(row.status, StageStatus::Done);
}

#[test]
fn stage_failure_records_failure_and_summary_status() {
    let mut view = initial_view();
    drop(update_launch_view(
        &mut view,
        LaunchMessage::StageFailed(failure()),
    ));
    assert_eq!(view.status, "container exited");
    assert!(!view.failure_ack);
    let stored = view.failure.as_ref().expect("failure stored");
    assert_eq!(stored.summary, "container exited");
    assert_eq!(
        view.stages
            .iter()
            .find(|row| row.stage == LaunchStage::Capsule)
            .expect("row")
            .status,
        StageStatus::Failed
    );
}

#[test]
fn acknowledging_failure_sets_ack_without_clearing_failure() {
    let mut view = initial_view();
    drop(update_launch_view(
        &mut view,
        LaunchMessage::StageFailed(failure()),
    ));
    drop(update_launch_view(
        &mut view,
        LaunchMessage::FailureAcknowledged,
    ));
    assert!(view.failure_ack);
    assert!(view.failure.is_some());
}

#[test]
fn advancing_to_running_stage_moves_active_index_with_transition() {
    let mut view = initial_view();
    assert_eq!(active_stage_index(&view), 0);
    update_stage(&mut view, LaunchStage::Identity, StageStatus::Done, "done");
    update_stage(
        &mut view,
        LaunchStage::Role,
        StageStatus::Running,
        "running",
    );
    assert_eq!(active_stage_index(&view), 1);
    let transition = view.label_transition.expect("transition recorded");
    assert_eq!((transition.from, transition.to), (0, 1));
}
