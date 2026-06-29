//! Launch cockpit update helpers.

use jackin_tui::components::StatusFooterHover;
use jackin_tui::runtime::UpdateResult;

use crate::tui::components::build_log_dialog::refresh_build_log_layout;
use crate::tui::effect::LaunchEffect;
use crate::tui::message::LaunchMessage;
use crate::tui::model::{LaunchStage, LaunchView, StageLabelTransition, StageStatus, StageView};

type LaunchUpdate = UpdateResult<LaunchEffect>;

#[must_use]
pub fn initial_view() -> LaunchView {
    LaunchView {
        identity: None,
        stages: LaunchStage::ALL
            .into_iter()
            .map(|stage| StageView {
                stage,
                status: StageStatus::Queued,
                detail: "queued".to_owned(),
            })
            .collect(),
        status: "preparing launch".to_owned(),
        failure: None,
        failure_ack: false,
        frame: 0,
        build_log_open: false,
        build_log_scroll: jackin_tui::scroll::TailScroll::default(),
        build_log_scroll_dragging: false,
        build_log_lines: Vec::new(),
        build_log_wrapped_lines: Vec::new(),
        build_log_wrapped_width: 0,
        build_log_viewport_height: 0,
        build_log_filled: 0,
        build_log_active: false,
        footer_hover: StatusFooterHover::default(),
        label_transition: None,
        failure_copy_hover: None,
        failure_copied: None,
        failure_revealed: None,
        failure_opened: None,
        container_info_open: false,
        container_info_copied: None,
        container_info_hover: None,
        container_info_scroll: jackin_tui::components::DialogBodyScroll::new(),
        quit_confirm: None,
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
            view.failure_revealed = None;
            view.failure_opened = None;
            view.failure = Some(failure);
        }
        LaunchMessage::FailureAcknowledged => {
            view.failure_ack = true;
            view.failure_copy_hover = None;
            view.failure_revealed = None;
            view.failure_opened = None;
        }
        LaunchMessage::FailureCopyHovered(target) => {
            view.failure_copy_hover = target;
        }
        LaunchMessage::FailureCopied(target) => {
            view.failure_copied = Some(target);
            view.failure_revealed = None;
            view.failure_opened = None;
        }
        LaunchMessage::FailureRevealed(target) => {
            view.failure_revealed = Some(target);
            view.failure_copied = None;
            view.failure_opened = None;
        }
        LaunchMessage::FailureOpened(target) => {
            view.failure_opened = Some(target);
            view.failure_copied = None;
            view.failure_revealed = None;
        }
        LaunchMessage::FooterHoverChanged(hover) => {
            view.footer_hover = hover;
        }
        LaunchMessage::BuildLogOpened => {
            view.build_log_open = true;
            view.build_log_scroll = jackin_tui::scroll::TailScroll::default();
            view.build_log_scroll_dragging = false;
            view.build_log_wrapped_lines.clear();
            view.build_log_wrapped_width = 0;
            view.build_log_viewport_height = 0;
            view.build_log_filled = 0;
            view.footer_hover.left = false;
        }
        LaunchMessage::BuildLogClosed => {
            view.build_log_open = false;
            view.build_log_scroll_dragging = false;
        }
        LaunchMessage::BuildLogScrolled { filled, delta } => {
            view.build_log_scroll.scroll_by(filled, delta);
        }
        LaunchMessage::BuildLogScrollSetFromTop { filled, top_offset } => {
            view.build_log_scroll =
                jackin_tui::scroll::TailScroll::new(filled.saturating_sub(top_offset.min(filled)));
        }
        LaunchMessage::BuildLogScrollDragChanged(dragging) => {
            view.build_log_scroll_dragging = dragging;
        }
        LaunchMessage::RenderTick {
            advance_frame,
            build_log_area,
            build_log_lines,
            build_log_active,
        } => {
            if advance_frame {
                view.frame = view.frame.wrapping_add(1);
            }
            let lines_changed = view.build_log_lines != build_log_lines;
            view.build_log_lines = build_log_lines;
            view.build_log_active = build_log_active;
            if let Some(area) = build_log_area {
                refresh_build_log_layout(view, area, lines_changed);
                view.build_log_scroll.clamp(view.build_log_filled);
            } else {
                view.build_log_wrapped_lines.clear();
                view.build_log_wrapped_width = 0;
                view.build_log_viewport_height = 0;
                view.build_log_filled = 0;
            }
        }
        LaunchMessage::ContainerInfoOpened => {
            view.container_info_open = true;
            view.container_info_copied = None;
            view.container_info_hover = None;
            view.container_info_scroll = jackin_tui::components::DialogBodyScroll::new();
            view.footer_hover.right = false;
        }
        LaunchMessage::ContainerInfoClosed => {
            view.container_info_open = false;
            view.container_info_copied = None;
            view.container_info_hover = None;
            view.footer_hover.right = false;
        }
        LaunchMessage::ContainerInfoCopied(row) => {
            view.container_info_copied = Some(row);
        }
        LaunchMessage::ContainerInfoHovered(row) => {
            view.container_info_hover = row;
        }
    }
    UpdateResult::redraw()
}

pub fn update_stage(view: &mut LaunchView, stage: LaunchStage, status: StageStatus, detail: &str) {
    let previous_active = active_stage_index(view);
    if let Some(row) = view.stages.iter_mut().find(|row| row.stage == stage) {
        row.status = status;
        row.detail = detail.to_owned();
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

#[cfg(test)]
mod tests;
