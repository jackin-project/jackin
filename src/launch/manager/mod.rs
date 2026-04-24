//! Workspace manager TUI — list, create, edit, delete workspaces from
//! the launcher. Reached via `m` from the Workspace picker stage.

pub mod agent_allow;
pub mod create;
pub mod github_mounts;
pub mod input;
pub mod mount_info;
pub mod render;
pub mod state;

pub use input::{InputOutcome, handle_key};
pub use render::render;
pub use state::{ManagerStage, ManagerState};
