//! Transitional root-console render/widget facades.

pub mod app;
pub(crate) mod auth_panel;
pub mod auth_kind;
mod create;
pub(crate) mod debug;
pub(crate) mod effect;
pub(crate) mod input;
mod launch;
pub mod message;
#[cfg(test)]
mod message_tests;
pub mod op_picker;
pub(crate) mod prompts;
pub mod render;
pub mod run;
pub mod state;

pub(crate) use crate::console::effects;
pub(crate) use crate::console::services::file_browser;
pub use app::{ConsoleStage, ConsoleState};
pub(crate) use input::{InputOutcome, handle_key};
pub(crate) use message::{ManagerMessage, update_manager};
pub use render::{prepare_for_render, render};
pub(crate) use run::consumes_letter_input;
pub use run::run_console;
#[cfg(test)]
pub(crate) use run::is_on_main_screen;
pub use state::{ManagerStage, ManagerState};
