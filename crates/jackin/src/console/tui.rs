//! Transitional root-console TUI facades.

pub(crate) mod input;
pub mod run;
pub mod state;

// ── Root-console type aliases ───────────────────────────────────────────────
// Concrete manager effect and save-flow types binding generic jackin-console
// effect vocabulary to root-binary-owned types.

pub(crate) type ManagerEffect = jackin_console::tui::effect::ConsoleManagerEffect<
    jackin_core::RoleSelector,
    jackin_config::RoleSource,
    jackin_core::OpRef,
>;

pub(crate) type FileBrowserEffectContext =
    jackin_console::tui::effect::FileBrowserEffectContext;

pub(crate) type WorkspaceSaveEffect = jackin_console::tui::effect::WorkspaceSaveEffect<
    jackin_config::MountConfig,
    state::PendingSaveCommit,
    jackin_runtime::isolation::state::IsolationRecord,
    jackin_config::WorkspaceConfig,
>;

pub(crate) type WorkspaceSaveWriteMode = jackin_console::tui::effect::WorkspaceSaveWriteMode;

pub(crate) type WorkspaceSaveWriteInput<'a> =
    jackin_console::tui::effect::WorkspaceSaveWriteInput<'a, jackin_config::WorkspaceConfig>;

pub use jackin_console::tui::console::{
    ConsoleStage, ConsoleState, new_console_state, new_console_state_with_op_available,
};
pub use input::{InputOutcome, handle_key};
pub use jackin_console::tui::launch::dispatch_launch_for_workspace;
pub use jackin_console::tui::view::{prepare_for_render, render};
pub(crate) use jackin_console::tui::state::update::{ManagerMessage, update_manager};
pub use run::run_console;
#[cfg(test)]
pub(crate) use jackin_console::tui::run::{is_on_main_screen, letter_input_state_for_console as letter_input_state};
pub use state::{ManagerStage, ManagerState};
