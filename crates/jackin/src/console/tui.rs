//! Transitional root-console TUI facades.

pub(crate) mod effect;
pub(crate) mod input;
pub mod message;
pub(crate) mod prompts;
pub mod run;
pub mod state;

pub use jackin_console::tui::console::{
    ConsoleStage, ConsoleState, new_console_state, new_console_state_with_op_available,
};
pub use input::{InputOutcome, handle_key};
pub use jackin_console::tui::launch::dispatch_launch_for_workspace;
pub use jackin_console::tui::view::{prepare_for_render, render};
pub(crate) use message::{ManagerMessage, update_manager};
pub use run::run_console;
#[cfg(test)]
pub(crate) use run::{is_on_main_screen, letter_input_state};
pub use state::{ManagerStage, ManagerState};
