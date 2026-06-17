//! Console session outcome types: what action the event loop should take when `run_console` returns.
//!
//! Not responsible for: executing the chosen action — callers pattern-match the
//! returned `ConsoleOutcome` and dispatch to the appropriate runtime handler.

use crate::selector::RoleSelector;
use crate::workspace::ResolvedWorkspace;

pub type ConsoleInstanceAction =
    jackin_console::tui::message::ConsoleInstanceAction<crate::agent::Agent>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleOutcome {
    Launch(RoleSelector, ResolvedWorkspace, Option<crate::agent::Agent>),
    InstanceAction {
        container: String,
        action: ConsoleInstanceAction,
    },
    /// Operator selected an agent AND a provider in the console picker.
    /// The chosen `Provider` drives the env redirection (e.g. Z.AI's
    /// Anthropic-compatible endpoint) and the tab-name suffix.
    NewSessionWithProvider {
        container: String,
        agent: crate::agent::Agent,
        provider: jackin_protocol::Provider,
    },
    /// Initial launch with a provider selected in the console before the
    /// container is created. The provider flows into the capsule's initial
    /// attach so the first session uses the chosen provider.
    LaunchWithProvider {
        selector: RoleSelector,
        workspace: ResolvedWorkspace,
        agent: crate::agent::Agent,
        provider: jackin_protocol::Provider,
    },
}

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
