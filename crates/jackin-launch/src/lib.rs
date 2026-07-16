//! jackin-launch: launch-progress presentation TUI.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`LaunchTui`] — launch progress UI.

pub mod animation;
pub mod build_log;
pub mod launch_output;
pub mod output;
pub mod progress;
pub mod standalone_dialog_sink;
pub mod tui;

pub use launch_output::LaunchTuiOutputSink;
pub use standalone_dialog_sink::install as install_standalone_dialog_sink;

pub use jackin_core::{
    FailureCopyTarget, FileDiff, LaunchCancelled, LaunchCandidate, LaunchDiagnostics,
    LaunchDialogResult, LaunchFailure, LaunchHostTerminal, LaunchIdentity, LaunchOutputSink,
    LaunchStage, LaunchTargetKind, PromptContextLine, StageLabelTransition, StageStatus, StageView,
    WorktreeInspect,
};
pub use tui::message::LaunchMessage;
pub use tui::model::{LaunchView, PromptResult};
pub use tui::update::{active_stage_index, initial_view, update_launch_view, update_stage};

mod test_support;
