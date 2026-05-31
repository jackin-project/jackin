//! Launch progress surface model and UI ownership.
//!
//! This crate is the extraction target for the launch cockpit. The first
//! boundary is the public launch model used by runtime orchestration; render,
//! update, and event-loop pieces move here in follow-up slices.

pub mod state;
pub mod tui;
pub mod update;

pub use state::{
    FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchMessage, LaunchStage, LaunchTargetKind,
    LaunchView, PromptResult, StageLabelTransition, StageStatus, StageView,
};
pub use update::{active_stage_index, initial_view, update_launch_view, update_stage};
