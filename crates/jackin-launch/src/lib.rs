//! Launch progress surface model and UI ownership.
//!
//! This crate is the extraction target for the launch cockpit. The first
//! boundary is the public launch model used by runtime orchestration; render,
//! update, and event-loop pieces move here in follow-up slices.

use std::path::{Path, PathBuf};

pub mod build_log;
pub mod state;
pub mod tui;
pub mod update;

pub use state::{
    FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchMessage, LaunchStage, LaunchTargetKind,
    LaunchView, PromptResult, StageLabelTransition, StageStatus, StageView,
};
pub use update::{active_stage_index, initial_view, update_launch_view, update_stage};

pub trait LaunchDiagnostics: Send + Sync {
    fn run_id(&self) -> &str;
    fn path(&self) -> &Path;
    fn command_output_path(&self, name: &str) -> PathBuf;
    fn compact(&self, kind: &str, message: &str);
    fn stage(&self, kind: &str, stage: &str, message: &str, detail: Option<&str>);
}

pub trait LaunchHostTerminal: Send + Sync {
    fn set_rich_surface_active(&self, active: bool);
    fn host_screen_owned(&self) -> bool;
    fn is_debug_mode(&self) -> bool;
    fn emit_compact_line(&self, kind: &str, line: &str);
}
