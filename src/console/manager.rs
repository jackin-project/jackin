//! Compatibility re-exports for integration tests and benchmarks.
//!
//! The public console TUI surface lives under `crate::console::tui`.
//! This module keeps the old `console::manager` import paths working.
pub use super::tui::app::new_console_state;
pub use super::tui::launch::dispatch_launch_for_workspace;
pub use super::tui::{ManagerStage, ManagerState, prepare_for_render, render};

pub type InputOutcome = jackin_console::tui::message::ConsoleInputOutcome<
    crate::selector::RoleSelector,
    crate::agent::Agent,
    crate::console::ConsoleInstanceAction,
    jackin_protocol::Provider,
>;

pub fn handle_key(
    state: &mut ManagerState<'_>,
    config: &mut crate::config::AppConfig,
    paths: &crate::paths::JackinPaths,
    cwd: &std::path::Path,
    key: crossterm::event::KeyEvent,
) -> anyhow::Result<super::tui::InputOutcome> {
    super::tui::handle_key(state, config, paths, cwd, key)
}

pub mod auth_kind {
    pub use jackin_console::tui::auth::AuthKind;
}

pub mod state {
    pub use crate::console::tui::state::{
        AuthFormTarget, AuthRow, CreatePreludeState, DragState, EditorState, EditorStateExt,
        EditorTab, FieldFocus, FileBrowserTarget, ManagerStage, ManagerState, Modal,
        SecretsScopeTag, SettingsState, TextInputTarget, auth_flat_rows, secrets_flat_rows,
    };
}
