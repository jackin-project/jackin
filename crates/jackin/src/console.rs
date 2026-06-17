//! jackin' interactive console (`jackin console`): TUI and domain logic.
//!
//! Entry point is `run_console`. The module tree is split into:
//!
//! * `domain` — pure data transforms and business rules (no side effects).
//! * `tui` — Elm Architecture state machine, input handling, and render loop.
//! * `services` — background async helpers (refresh, drift check, etc.).
//! * `effects` — side-effecting operations dispatched by the event loop.
//!
//! Not responsible for: the rendering primitives themselves (those live in
//! `jackin-console` and `jackin-tui` crates).

// `ConsoleStage` collapsed to a single variant in PR #171's Modal::RolePicker
// cleanup. The module is kept as-is (with `if let ConsoleStage::Manager(_)`
// patterns) so a future stage can be added without rewriting every match
// site. The irrefutable-pattern lint is allowed at the module level rather
// than peppering individual sites.
#![allow(irrefutable_let_patterns)]

mod domain;
pub mod effects;
pub mod manager;
mod preview;
mod services;
pub mod terminal;
pub mod tui;

#[cfg(test)]
mod tests;

pub use domain::{WorkspaceChoice, build_workspace_choice};
pub use terminal::TerminalSession;
pub use tui::{ConsoleStage, ConsoleState, run_console};

pub type ConsoleInstanceAction =
    jackin_console::tui::message::ConsoleInstanceAction<crate::agent::Agent>;
pub type ConsoleOutcome = jackin_console::tui::message::ConsoleOutcome<
    crate::selector::RoleSelector,
    crate::workspace::ResolvedWorkspace,
    crate::agent::Agent,
    jackin_protocol::Provider,
>;
pub use jackin_console::tui::message::InstanceActionHandler;
