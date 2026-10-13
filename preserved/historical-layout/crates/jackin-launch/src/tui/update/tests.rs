// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `update`.
use super::*;
use crate::{LaunchFailure, LaunchTargetKind};

fn identity() -> crate::LaunchIdentity {
    crate::LaunchIdentity {
        role: "architect".into(),
        agent: "claude".into(),
        target_kind: LaunchTargetKind::Workspace,
        target_label: "demo".into(),
        mounts: Vec::new(),
        image: None,
        container: None,
    }
}

#[test]
fn failure_acknowledged_clears_hover_and_sets_ack() {
    let mut view = initial_view();
    drop(update_launch_view(
        &mut view,
        LaunchMessage::Started(identity()),
    ));
    let _unused = update_launch_view(
        &mut view,
        LaunchMessage::StageFailed(LaunchFailure {
            title: "Build failed".into(),
            summary: "docker build failed".into(),
            detail: None,
            next_step: None,
            stage: LaunchStage::DerivedImage,
        }),
    );
    view.failure_copy_hover = Some(crate::FailureCopyTarget::RunId);

    drop(update_launch_view(
        &mut view,
        LaunchMessage::FailureAcknowledged,
    ));

    assert!(view.failure_ack);
    assert_eq!(view.failure_copy_hover, None);
}

#[test]
fn stage_failed_clears_overlays_so_failure_cannot_be_hidden() {
    let mut view = initial_view();
    view.build_log_open = true;
    view.build_log_scroll_dragging = true;
    view.container_info_open = true;

    drop(update_launch_view(
        &mut view,
        LaunchMessage::StageFailed(LaunchFailure {
            title: "Build failed".into(),
            summary: "docker build failed".into(),
            detail: None,
            next_step: None,
            stage: LaunchStage::DerivedImage,
        }),
    ));

    assert!(
        !view.build_log_open,
        "build-log overlay must close on failure"
    );
    assert!(
        !view.build_log_scroll_dragging,
        "build-log drag binding must release on failure"
    );
    assert!(
        !view.container_info_open,
        "container-info overlay must close on failure"
    );
    assert!(view.failure.is_some(), "failure must be set");
    assert!(!view.failure_ack, "failure must start unacknowledged");
    assert_eq!(
        view.failure_scroll,
        termrock::scroll::DialogScroll::new(),
        "failure body scroll resets for a fresh failure"
    );
}

#[test]
fn failure_copy_messages_track_hover_and_copied_target() {
    let mut view = initial_view();

    let _unused = update_launch_view(
        &mut view,
        LaunchMessage::FailureCopyHovered(Some(crate::FailureCopyTarget::RunId)),
    );

    assert_eq!(
        view.failure_copy_hover,
        Some(crate::FailureCopyTarget::RunId)
    );

    let _unused = update_launch_view(
        &mut view,
        LaunchMessage::FailureCopied(crate::FailureCopyTarget::RunId),
    );

    assert_eq!(view.failure_copied, Some(crate::FailureCopyTarget::RunId));
}

#[test]
fn footer_hover_message_replaces_hover_state() {
    let mut view = initial_view();

    let _unused = update_launch_view(
        &mut view,
        LaunchMessage::FooterHoverChanged(StatusFooterHover {
            left: true,
            usage: false,
            right: false,
            right_debug: false,
        }),
    );

    assert!(view.footer_hover.left);
    assert!(!view.footer_hover.right);
}

#[test]
fn build_log_messages_open_reset_and_close_overlay() {
    let mut view = initial_view();
    view.footer_hover.left = true;
    view.build_log_scroll = termrock::scroll::TailScroll::new(4);

    drop(update_launch_view(&mut view, LaunchMessage::BuildLogOpened));

    assert!(view.build_log_open);
    assert_eq!(view.build_log_scroll.offset(), 0);
    assert!(!view.footer_hover.left);

    drop(update_launch_view(&mut view, LaunchMessage::BuildLogClosed));

    assert!(!view.build_log_open);
}

#[test]
fn build_log_scroll_message_updates_tail_offset() {
    let mut view = initial_view();

    let _unused = update_launch_view(
        &mut view,
        LaunchMessage::BuildLogScrolled {
            filled: 12,
            delta: 3,
        },
    );

    assert_eq!(view.build_log_scroll.offset(), 3);
}

#[test]
fn build_log_scroll_set_from_top_converts_to_tail_offset() {
    let mut view = initial_view();

    let _unused = update_launch_view(
        &mut view,
        LaunchMessage::BuildLogScrollSetFromTop {
            filled: 12,
            top_offset: 9,
        },
    );

    assert_eq!(view.build_log_scroll.offset(), 3);
}

#[test]
fn build_log_scroll_drag_state_resets_when_overlay_closes() {
    let mut view = initial_view();

    drop(update_launch_view(
        &mut view,
        LaunchMessage::BuildLogScrollDragChanged(true),
    ));
    assert!(view.build_log_scroll_dragging);

    drop(update_launch_view(&mut view, LaunchMessage::BuildLogClosed));

    assert!(!view.build_log_scroll_dragging);
}

#[test]
fn render_tick_advances_frame_and_clamps_build_log_scroll() {
    let mut view = initial_view();
    view.build_log_scroll = termrock::scroll::TailScroll::new(99);

    let _unused = update_launch_view(
        &mut view,
        LaunchMessage::RenderTick {
            advance_frame: true,
            build_log_area: Some(ratatui::layout::Rect::new(0, 0, 40, 8)),
            build_log_lines: (0..20).map(|idx| format!("line {idx}")).collect(),
            build_log_active: true,
        },
    );

    assert_eq!(view.frame, 1);
    assert!(view.build_log_filled > 0);
    assert!(view.build_log_scroll.offset() <= view.build_log_filled);
    assert_eq!(view.build_log_lines.len(), 20);
    assert!(!view.build_log_wrapped_lines.is_empty());
    assert!(view.build_log_active);
}

#[test]
fn container_info_messages_open_copy_and_close_overlay() {
    let mut view = initial_view();
    view.footer_hover.right = true;
    view.container_info_copied = Some(4);

    drop(update_launch_view(
        &mut view,
        LaunchMessage::ContainerInfoOpened,
    ));

    assert!(view.container_info_open);
    assert_eq!(view.container_info_copied, None);
    assert!(!view.footer_hover.right);

    drop(update_launch_view(
        &mut view,
        LaunchMessage::ContainerInfoCopied(2),
    ));

    assert_eq!(view.container_info_copied, Some(2));

    drop(update_launch_view(
        &mut view,
        LaunchMessage::ContainerInfoClosed,
    ));

    assert!(!view.container_info_open);
    assert_eq!(view.container_info_copied, None);
    assert!(!view.footer_hover.right);
}
