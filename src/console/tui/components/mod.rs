//! Reusable widgets for the workspace manager TUI.
//!
//! Shared TUI components are promoted into `jackin-tui`; this module keeps
//! host-console facades and still-local widgets while the architecture
//! migration proceeds.

pub mod auth_panel;
pub(crate) mod editor_footer;
pub(crate) mod modal_footer;
pub mod op_picker;
pub(crate) mod settings_footer;

#[cfg(test)]
mod consistency_tests;
