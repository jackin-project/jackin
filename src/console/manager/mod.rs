//! Workspace manager TUI — list, create, edit, delete workspaces from
//! the operator console. Reached via `m` from the Workspace picker stage.

pub(crate) mod effects;
pub(crate) mod file_browser;
pub mod message;
#[cfg(test)]
mod message_tests;
pub mod state;

pub use crate::console::tui::render::render;
pub use crate::console::tui::input::{InputOutcome, handle_key};
pub(crate) use effects::poll_background_messages;
pub(crate) use message::{ManagerMessage, update_manager};
pub use crate::console::tui::render::prepare_for_render;
pub use state::{ManagerStage, ManagerState};

impl jackin_console::github_mounts::WorkspaceMounts for crate::workspace::WorkspaceConfig {
    fn mount_sources(&self) -> impl Iterator<Item = &str> {
        self.mounts.iter().map(|mount| mount.src.as_str())
    }
}
