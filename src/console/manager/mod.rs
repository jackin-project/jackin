//! Workspace manager TUI — list, create, edit, delete workspaces from
//! the operator console. Reached via `m` from the Workspace picker stage.

mod agent_allow;
mod create;
mod github_mounts;
pub mod input;
pub mod mount_info;
pub mod render;
pub mod state;

pub use input::{InputOutcome, handle_key};
pub use render::render;
pub use state::{ManagerStage, ManagerState};
