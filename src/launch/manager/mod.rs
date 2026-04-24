//! Workspace manager TUI — list, create, edit, delete workspaces from
//! the launcher. Reached via `m` from the Workspace picker stage.

pub mod create;
pub mod render;
pub mod state;

pub use render::render;
pub use state::{ManagerStage, ManagerState};
