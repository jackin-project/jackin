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

/// Callback invoked for `runs_in_place` actions.
///
/// The handler performs the docker work (eject, purge). Making it async lets
/// the caller `.await` the work on the existing runtime without building a
/// separate runtime, so the reactor can service other tasks between awaits
/// while Docker/git calls are in flight.
pub trait InstanceActionHandler {
    async fn run_in_place(
        &mut self,
        container: &str,
        action: ConsoleInstanceAction,
    ) -> anyhow::Result<()>;
}
