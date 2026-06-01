//! Reusable widgets for the workspace manager TUI.
//!
//! Shared TUI components are promoted into `jackin-tui`; this module keeps
//! host-console facades and still-local widgets while the architecture
//! migration proceeds.

pub mod op_picker;

#[cfg(test)]
mod consistency_tests;
