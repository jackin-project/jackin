//! Launch cockpit message vocabulary.

use jackin_tui::components::StatusFooterHover;
use ratatui::layout::Rect;

use crate::tui::model::{
    FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchStage, StageStatus,
};

#[derive(Debug, Clone)]
pub enum LaunchMessage {
    Started(LaunchIdentity),
    IdentityUpdated(LaunchIdentity),
    StageStatus {
        stage: LaunchStage,
        status: StageStatus,
        detail: String,
        set_activity: bool,
    },
    StageFailed(LaunchFailure),
    FailureAcknowledged,
    FailureCopyHovered(Option<FailureCopyTarget>),
    FailureCopied(FailureCopyTarget),
    FailureRevealed(FailureCopyTarget),
    FailureOpened(FailureCopyTarget),
    FooterHoverChanged(StatusFooterHover),
    BuildLogOpened,
    BuildLogClosed,
    BuildLogScrolled {
        filled: usize,
        delta: isize,
    },
    BuildLogScrollSetFromTop {
        filled: usize,
        top_offset: usize,
    },
    BuildLogScrollDragChanged(bool),
    RenderTick {
        advance_frame: bool,
        build_log_area: Option<Rect>,
        build_log_lines: Vec<String>,
        build_log_active: bool,
    },
    ContainerInfoOpened,
    ContainerInfoClosed,
    ContainerInfoCopied(usize),
    ContainerInfoHovered(Option<usize>),
}
