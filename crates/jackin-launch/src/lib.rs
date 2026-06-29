//! Launch progress surface model and UI ownership.
//!
//! This crate owns the launch cockpit boundary. Non-visual launch
//! orchestration lives in `progress`, build-log capture lives in `build_log`,
//! and model/message/update/run/view code lives under `tui`.

use std::path::{Path, PathBuf};

pub mod build_log;
pub mod progress;
pub mod tui;

pub use tui::app::{
    FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchStage, LaunchTargetKind, LaunchView,
    PromptContextLine, PromptResult, StageLabelTransition, StageStatus, StageView,
};
pub use tui::message::LaunchMessage;
pub use tui::update::{active_stage_index, initial_view, update_launch_view, update_stage};

/// One changed file entry for the D24 Inspect surface.
#[derive(Debug, Clone)]
pub struct FileDiff {
    /// Porcelain status character (`M`, `A`, `D`, `?`, …).
    pub status: char,
    /// Path relative to the worktree root.
    pub path: String,
    /// File content at HEAD — `None` for added/untracked files.
    pub before: Option<String>,
    /// File content in the working tree — `None` for deleted files.
    pub after: Option<String>,
}

/// Pre-computed inspection data for one worktree in the D24 surface.
#[derive(Debug, Clone)]
pub struct WorktreeInspect {
    /// Display label shown in the repos pane (workspace name or mount path).
    pub label: String,
    /// Changed files with their diff content.
    pub files: Vec<FileDiff>,
}

/// One candidate row in the D23 launch dialog.
#[derive(Debug, Clone)]
pub struct LaunchCandidate {
    /// Formatted label shown in the picker list.
    pub label: String,
    /// `true` if the candidate has dirty/unpushed state.
    /// Dirty candidates require a `ConfirmDialog` before deletion (D21).
    pub is_dirty: bool,
    /// Pre-fetched inspect data (one entry per isolated worktree in this
    /// instance). Empty for clean/crashed instances with no worktree state.
    pub inspect: Vec<WorktreeInspect>,
}

/// Outcome of the D23 launch dialog.
#[derive(Debug, Clone)]
pub enum LaunchDialogResult {
    /// Operator chose to start a new instance.
    StartFresh,
    /// Operator chose to restore the candidate at this index.
    Restore(usize),
    /// Operator confirmed deletion of the candidate at this index.
    Delete(usize),
}

/// Marker error: the operator deliberately aborted the launch (Ctrl+C,
/// Ctrl+Q, or a Cancel modal). This is an intent, not a failure — the binary
/// entry point treats it as a clean exit and never renders it as `error:`.
///
/// Carried as a concrete error inside an `anyhow::Error` so any layer can
/// detect it via [`LaunchCancelled::is_cancel`] regardless of `.context(..)`
/// wrapping. `Display` keeps the historical "launch cancelled by operator"
/// wording for debug/log surfaces.
#[derive(Debug)]
pub struct LaunchCancelled;

impl std::fmt::Display for LaunchCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("launch cancelled by operator")
    }
}

impl std::error::Error for LaunchCancelled {}

impl LaunchCancelled {
    /// Build the cancellation as an `anyhow::Error` for return up the stack.
    pub fn err() -> anyhow::Error {
        anyhow::Error::new(Self)
    }

    /// `true` if `error` — or anything in its source chain — is a
    /// `LaunchCancelled`. `anyhow`'s downcast walks the chain, so the check
    /// survives intermediate `.context(..)` layers.
    pub fn is_cancel(error: &anyhow::Error) -> bool {
        error.downcast_ref::<Self>().is_some()
    }
}

pub trait LaunchDiagnostics: Send + Sync {
    fn run_id(&self) -> &str;
    fn path(&self) -> &Path;
    fn command_output_path(&self, name: &str) -> PathBuf;
    fn compact(&self, kind: &str, message: &str);
    fn stage(&self, kind: &str, stage: &str, message: &str, detail: Option<&str>);
}

impl LaunchDiagnostics for jackin_diagnostics::RunDiagnostics {
    fn run_id(&self) -> &str {
        self.run_id()
    }

    fn path(&self) -> &Path {
        self.path()
    }

    fn command_output_path(&self, name: &str) -> PathBuf {
        self.command_output_path(name)
    }

    fn compact(&self, kind: &str, message: &str) {
        self.compact(kind, message);
    }

    fn stage(&self, kind: &str, stage: &str, message: &str, detail: Option<&str>) {
        self.stage(kind, stage, message, detail);
    }
}

pub trait LaunchHostTerminal: Send + Sync {
    fn set_rich_surface_active(&self, active: bool);
    fn host_screen_owned(&self) -> bool;
    fn is_debug_mode(&self) -> bool;
    fn emit_compact_line(&self, kind: &str, line: &str);
    fn emit_debug_line(&self, category: &str, line: &str);
    fn set_pointer_shape(&self, pointer: bool);
    fn copy_to_clipboard(&self, payload: &str) -> bool;
    fn reveal_file(&self, path: &Path) -> bool;
    fn open_file(&self, path: &Path) -> bool;
}

mod test_support {
    use super::LaunchHostTerminal;

    struct TestHostTerminal;

    impl LaunchHostTerminal for TestHostTerminal {
        fn set_rich_surface_active(&self, _active: bool) {}
        fn host_screen_owned(&self) -> bool {
            false
        }
        fn is_debug_mode(&self) -> bool {
            false
        }
        fn emit_compact_line(&self, _kind: &str, _line: &str) {}
        fn emit_debug_line(&self, _category: &str, _line: &str) {}
        fn set_pointer_shape(&self, _pointer: bool) {}
        fn copy_to_clipboard(&self, _payload: &str) -> bool {
            true
        }
        fn reveal_file(&self, _path: &std::path::Path) -> bool {
            false
        }
        fn open_file(&self, _path: &std::path::Path) -> bool {
            false
        }
    }

    static TEST_HOST_TERMINAL: TestHostTerminal = TestHostTerminal;

    pub(crate) fn test_host_terminal() -> &'static dyn LaunchHostTerminal {
        &TEST_HOST_TERMINAL
    }
}
