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
    fn set_pointer_shape(&self, pointer: bool);
    fn copy_to_clipboard(&self, payload: &str) -> bool;
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
        fn set_pointer_shape(&self, _pointer: bool) {}
        fn copy_to_clipboard(&self, _payload: &str) -> bool {
            true
        }
    }

    static TEST_HOST_TERMINAL: TestHostTerminal = TestHostTerminal;

    pub(crate) fn test_host_terminal() -> &'static dyn LaunchHostTerminal {
        &TEST_HOST_TERMINAL
    }
}
