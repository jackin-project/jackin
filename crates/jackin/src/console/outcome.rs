//! Console session outcome types: what action the event loop should take when `run_console` returns.
//!
//! Not responsible for: executing the chosen action — callers pattern-match the
//! returned `ConsoleOutcome` and dispatch to the appropriate runtime handler.

use crate::selector::RoleSelector;
use crate::workspace::ResolvedWorkspace;

pub type ConsoleInstanceAction =
    jackin_console::tui::message::ConsoleInstanceAction<crate::agent::Agent>;

pub type ConsoleOutcome = jackin_console::tui::message::ConsoleOutcome<
    RoleSelector,
    ResolvedWorkspace,
    crate::agent::Agent,
    jackin_protocol::Provider,
>;
pub use jackin_console::tui::message::InstanceActionHandler;
