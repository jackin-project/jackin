//! Transitional root-console TUI facades.

pub mod app;
pub(crate) mod components;
pub(crate) mod debug;
pub(crate) mod effect;
pub(crate) mod input;
pub(crate) mod instance_action;
pub mod launch;
pub(crate) mod layout;
pub mod message;
pub(crate) mod op_picker;
pub(crate) mod prompts;
pub mod run;
pub mod state;
pub mod view;

pub use app::{ConsoleStage, ConsoleState, new_console_state, new_console_state_with_op_available};
pub use input::{InputOutcome, handle_key};
pub use layout::prepare_for_render;
pub(crate) use message::{ManagerMessage, update_manager};
pub(crate) use run::consumes_letter_input;
#[cfg(test)]
pub(crate) use run::is_on_main_screen;
pub use run::run_console;
pub use state::{ManagerStage, ManagerState};
pub use view::render;
