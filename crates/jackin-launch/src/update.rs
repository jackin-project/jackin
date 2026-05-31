//! Launch cockpit update helpers.

use jackin_tui::components::StatusFooterHover;
use jackin_tui::runtime::{NoEffect, UpdateResult};

use crate::state::{
    LaunchMessage, LaunchStage, LaunchView, StageLabelTransition, StageStatus, StageView,
};

type LaunchUpdate = UpdateResult<NoEffect>;

#[must_use]
pub fn initial_view() -> LaunchView {
    LaunchView {
        identity: None,
        stages: LaunchStage::ALL
            .into_iter()
            .map(|stage| StageView {
                stage,
                status: StageStatus::Queued,
                detail: "queued".to_string(),
            })
            .collect(),
        status: "preparing launch".to_string(),
        failure: None,
        failure_ack: false,
        frame: 0,
        build_log_open: false,
        build_log_scroll: jackin_tui::scroll::TailScroll::default(),
        footer_hover: StatusFooterHover::default(),
        label_transition: None,
        failure_copy_hover: None,
        failure_copied: None,
        container_info_open: false,
        container_info_copied: None,
    }
}

pub fn update_launch_view(view: &mut LaunchView, msg: LaunchMessage) -> LaunchUpdate {
    match msg {
        LaunchMessage::Started(identity) => {
            let preposition = identity.target_kind.launch_preposition();
            view.status = format!("loading {} {preposition}", identity.role);
            view.identity = Some(identity);
        }
        LaunchMessage::IdentityUpdated(identity) => {
            view.identity = Some(identity);
        }
        LaunchMessage::StageStatus {
            stage,
            status,
            detail,
            set_activity,
        } => {
            update_stage(view, stage, status, &detail);
            if set_activity {
                view.status = detail;
            }
        }
        LaunchMessage::StageFailed(failure) => {
            let stage = failure.stage;
            let summary = failure.summary.clone();
            update_stage(view, stage, StageStatus::Failed, &summary);
            view.status = summary;
            view.failure_ack = false;
            view.failure_copy_hover = None;
            view.failure_copied = None;
            view.failure = Some(failure);
        }
    }
    UpdateResult::redraw()
}

pub fn update_stage(view: &mut LaunchView, stage: LaunchStage, status: StageStatus, detail: &str) {
    let previous_active = active_stage_index(view);
    if let Some(row) = view.stages.iter_mut().find(|row| row.stage == stage) {
        row.status = status;
        row.detail = detail.to_string();
    }
    let next_active = active_stage_index(view);
    if previous_active != next_active {
        view.label_transition = Some(StageLabelTransition {
            from: previous_active,
            to: next_active,
            start_frame: view.frame,
        });
    }
}

#[must_use]
pub fn active_stage_index(view: &LaunchView) -> usize {
    if let Some(failed) = view
        .stages
        .iter()
        .position(|row| row.status == StageStatus::Failed)
    {
        return failed;
    }

    let first_incomplete = view
        .stages
        .iter()
        .position(|row| !matches!(row.status, StageStatus::Done | StageStatus::Skipped));
    let Some(frontier) = first_incomplete else {
        return view.stages.len().saturating_sub(1);
    };
    if view.stages[frontier].status == StageStatus::Running {
        return frontier;
    }

    view.stages
        .iter()
        .position(|row| row.status == StageStatus::Running)
        .filter(|running| *running < frontier)
        .unwrap_or_else(|| frontier.saturating_sub(1))
}
