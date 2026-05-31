//! Workspace manager TUI — list, create, edit, delete workspaces from
//! the operator console. Reached via `m` from the Workspace picker stage.

pub mod agent_allow;
pub mod auth_kind;
mod create;
pub(crate) mod github_mounts;
pub mod input;
pub mod message;
pub mod mount_diff;
pub mod mount_info;
pub mod mount_info_cache;
pub mod state;

pub use crate::console::tui::render;
pub use input::{InputOutcome, handle_key};
pub(crate) use message::{ManagerMessage, update_manager};
pub use render::prepare_for_render;
pub use render::render;
pub use state::{ManagerStage, ManagerState};
