//! Thin re-export shell: concrete ConsoleState/ConsoleStage aliases and
//! constructors now live in jackin-console.

pub use jackin_console::tui::console::{
    ConsoleStage, ConsoleState, new_console_state, new_console_state_with_op_available,
    new_console_state_with_startup_error,
};

// Pulled into the `tests` child via `use super::*`.
#[cfg(test)]
use jackin_config::AppConfig;

#[cfg(test)]
mod tests;
