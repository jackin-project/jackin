//! Transitional root-console render/widget facades.

pub mod auth_kind;
pub mod components;
mod create;
pub(crate) mod input;
pub mod message;
#[cfg(test)]
mod message_tests;
pub mod render;
pub mod state;

pub(crate) use crate::console::effects;
pub(crate) use crate::console::services::file_browser;
pub(crate) use input::{InputOutcome, handle_key};
pub(crate) use message::{ManagerMessage, update_manager};
pub use render::{prepare_for_render, render};
pub use state::{ManagerStage, ManagerState};
