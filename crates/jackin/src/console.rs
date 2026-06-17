//! Binary adapter for the jackin' interactive console (`jackin console`).
//!
//! Entry point is `run_console`. Long-term console product behavior belongs in
//! `jackin-console`; this root module should only bind that surface to the
//! binary crate's concrete config, Docker, runtime, terminal, and CLI services.
//! The remaining root module tree is transitional and split into:
//!
//! * `domain` — root-bound data transforms that still use binary-owned types.
//! * `tui` — transitional state/input/render adapters over `jackin-console`.
//! * `services` — root IO adapters (config, Docker, runtime, op, tokens).
//! * `effects` — interpreter for non-TUI work requested by the event loop.
//!
//! New console state, input, update, rendering, dialog, and product-decision
//! logic should move to `jackin-console` or a lower shared crate unless it
//! needs root-only IO or command/runtime integration.

// `ConsoleStage` collapsed to a single variant in PR #171's Modal::RolePicker
// cleanup. The module is kept as-is (with `if let ConsoleStage::Manager(_)`
// patterns) so a future stage can be added without rewriting every match
// site. The irrefutable-pattern lint is allowed at the module level rather
// than peppering individual sites.
#![allow(irrefutable_let_patterns)]

mod domain;
pub mod effects;
mod services;
pub mod terminal;
pub mod tui;

#[cfg(test)]
mod tests;

pub use domain::{WorkspaceChoice, build_workspace_choice};
pub use terminal::TerminalSession;
pub use tui::{ConsoleStage, ConsoleState, run_console};

pub type ConsoleInstanceAction =
    jackin_console::tui::message::ConsoleInstanceAction<jackin_core::Agent>;
pub type ConsoleOutcome = jackin_console::tui::message::ConsoleOutcome<
    crate::selector::RoleSelector,
    crate::workspace::ResolvedWorkspace,
    jackin_core::Agent,
    jackin_protocol::Provider,
>;
pub use jackin_console::tui::message::InstanceActionHandler;
