//! Workspace-manager message/update boundary — thin re-export shell.
//!
//! Concrete type aliases and the `update_manager` reducer live in
//! `jackin-console::tui::state::update`; this module re-exports the full
//! public surface for callers in the root binary crate.

pub(crate) use jackin_console::tui::state::update::{
    ManagerBackgroundEvent, ManagerMessage, update_manager,
};


#[cfg(test)]
mod tests;
