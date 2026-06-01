//! Launch cockpit message vocabulary.

use jackin_tui::components::StatusFooterHover;

use crate::tui::app::{FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchStage, StageStatus};

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
    FooterHoverChanged(StatusFooterHover),
    BuildLogOpened,
    BuildLogClosed,
    BuildLogScrolled {
        filled: usize,
        delta: isize,
    },
    RenderTick {
        advance_frame: bool,
        build_log_filled: Option<usize>,
        build_log_lines: Vec<String>,
        build_log_active: bool,
    },
    ContainerInfoOpened,
    ContainerInfoClosed,
    ContainerInfoCopied(usize),
}
