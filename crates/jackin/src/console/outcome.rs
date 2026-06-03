//! Console session outcome types: what action the event loop should take when `run_console` returns.
//!
//! Not responsible for: executing the chosen action — callers pattern-match the
//! returned `ConsoleOutcome` and dispatch to the appropriate runtime handler.

use crate::selector::RoleSelector;
use crate::workspace::ResolvedWorkspace;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleInstanceAction {
    Reconnect,
    /// Reconnect and ask the in-container daemon to focus this
    /// pane (`session_id`) before forwarding output. Carries through
    /// to `attach::reconnect_or_create_session_with_focus` which
    /// appends the `--focus <id>` flag on the `docker exec`.
    ReconnectFocus(u64),
    NewSession,
    NewSessionWithAgent(crate::agent::Agent),
    Shell,
    Inspect,
    Stop,
    Purge,
}

impl ConsoleInstanceAction {
    /// Actions that don't replace the TUI with another foreground process
    /// (Stop/Purge) run inside the console event loop via
    /// `InstanceActionHandler`. The rest tear down the TUI so the launched
    /// container/agent can own the terminal.
    pub const fn runs_in_place(self) -> bool {
        matches!(self, Self::Stop | Self::Purge)
    }
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
