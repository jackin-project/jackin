//! Workspace manager TUI — list, create, edit, delete workspaces from
//! the operator console. Reached via `m` from the Workspace picker stage.

pub mod agent_allow;
pub mod auth_kind;
pub mod auth_rows;
mod create;
pub(crate) mod editor_footer;
pub(crate) mod editor_geometry;
pub mod input;
pub(crate) mod list_geometry;
pub mod message;
pub(crate) mod modal_footer;
pub(crate) mod modal_layout;
pub mod mount_diff;
pub(crate) mod mount_display;
pub(crate) mod op_breadcrumb;
mod pre_render;
pub(crate) mod settings_footer;
pub(crate) mod settings_geometry;
pub mod state;
pub mod workspace_summary;

pub use crate::console::tui::render::render;
pub use input::{InputOutcome, handle_key};
pub(crate) use message::{ManagerMessage, poll_background_messages, update_manager};
pub use pre_render::prepare_for_render;
pub use state::{ManagerStage, ManagerState};

impl jackin_console::github_mounts::WorkspaceMounts for crate::workspace::WorkspaceConfig {
    fn mount_sources(&self) -> impl Iterator<Item = &str> {
        self.mounts.iter().map(|mount| mount.src.as_str())
    }
}
