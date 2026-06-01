//! Workspace manager TUI — list, create, edit, delete workspaces from
//! the operator console. Reached via `m` from the Workspace picker stage.

pub(crate) mod auth_panel;
pub mod auth_kind;
mod create;
pub(crate) mod editor_footer;
pub(crate) mod editor_geometry;
pub(crate) mod effects;
pub(crate) mod file_browser;
pub mod input;
pub(crate) mod list_geometry;
pub mod message;
pub(crate) mod modal_footer;
pub(crate) mod modal_layout;
pub(crate) mod mount_display;
mod pre_render;
pub(crate) mod settings_footer;
pub(crate) mod settings_geometry;
pub mod state;

pub use crate::console::tui::render::render;
pub use input::{InputOutcome, handle_key};
pub(crate) use effects::poll_background_messages;
pub(crate) use message::{ManagerMessage, update_manager};
pub use pre_render::prepare_for_render;
pub use state::{ManagerStage, ManagerState};

impl jackin_console::github_mounts::WorkspaceMounts for crate::workspace::WorkspaceConfig {
    fn mount_sources(&self) -> impl Iterator<Item = &str> {
        self.mounts.iter().map(|mount| mount.src.as_str())
    }
}
