//! Manager state machine for the jackin' console TUI.
//!
//! `ManagerState` and all concrete type aliases now live in `jackin-console`.
//! This module re-exports the full public surface.

pub use jackin_console::tui::state::*;

// These re-imports are used by the child `tests` module via `use super::*`.
// Child modules have access to private items of their parent, so placing them
// here (even without `pub`) makes them available to tests without polluting
// the crate's public API.
#[cfg(test)]
use jackin_config::AppConfig;
#[cfg(test)]
use jackin_console::tui::auth::AuthKind;
#[cfg(test)]
use jackin_core::EnvValue;

#[cfg(test)]
mod tests;
