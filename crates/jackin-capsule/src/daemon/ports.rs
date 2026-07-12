//! Daemon port seams for control, attach, status, and persistence.
//!
//! These thin traits document the module boundaries used by characterization
//! tests and the proptest-style session state-machine sim (R-daemon-decomp /
//! R-sim-turmoil). Production code continues to call concrete Multiplexer
//! methods; the traits exist so tests can inject fakes at phase boundaries
//! without standing up the full select-loop.

/// Control-channel port: one-shot status / usage / runtime-event replies.
pub(crate) trait ControlPort {
    /// Whether a control request for `session_id` should ACK without mutation.
    fn control_acks_unknown_session(&self, session_id: &str) -> bool;
}

/// Attach lifecycle: single active client; Hello displaces the previous client.
pub(crate) trait AttachPort {
    /// True when a second Hello must displace the current attach client.
    fn should_displace_on_hello(&self, has_active_client: bool) -> bool;
}

/// Status publication surface (tab labels, session list).
pub(crate) trait StatusPort {
    /// True when an exited session should retire its codename from labels.
    fn should_retire_codename_on_exit(&self, remaining_live: usize) -> bool;
}

/// Persistence / reattach decisions after last-session exit.
pub(crate) trait PersistencePort {
    /// True when last-session exit handling should be deferred (dialog open).
    fn defer_last_session_exit(&self, dialog_open: bool) -> bool;
}

/// Default production-shaped port implementations (pure rules).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DefaultDaemonPorts;

impl ControlPort for DefaultDaemonPorts {
    fn control_acks_unknown_session(&self, session_id: &str) -> bool {
        !session_id.is_empty()
    }
}

impl AttachPort for DefaultDaemonPorts {
    fn should_displace_on_hello(&self, has_active_client: bool) -> bool {
        has_active_client
    }
}

impl StatusPort for DefaultDaemonPorts {
    fn should_retire_codename_on_exit(&self, remaining_live: usize) -> bool {
        // Always retire the exited session's label; remaining_live is for
        // last-session drain classification (INV-D8 / INV-D19).
        let _ = remaining_live;
        true
    }
}

impl PersistencePort for DefaultDaemonPorts {
    fn defer_last_session_exit(&self, dialog_open: bool) -> bool {
        super::should_defer_last_session_exit(dialog_open)
    }
}

/// Minimal session lifecycle state machine for the proptest-style sim lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionSmState {
    /// No sessions; daemon idle / may exit.
    Empty,
    /// At least one live session.
    Live { count: usize },
    /// Last session exited; dirty-exit dialog may be open.
    Draining { dialog_open: bool },
    /// Daemon exit requested.
    Exited,
}

/// Events that drive the session lifecycle SM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionSmEvent {
    SessionSpawned,
    SessionExited,
    DialogOpened,
    DialogClosed,
    DrainCompleted,
}

/// Step the session lifecycle SM (deterministic; used by sim tests).
pub(crate) fn session_sm_step(state: SessionSmState, event: SessionSmEvent) -> SessionSmState {
    match (state, event) {
        (SessionSmState::Empty, SessionSmEvent::SessionSpawned) => {
            SessionSmState::Live { count: 1 }
        }
        (SessionSmState::Live { count }, SessionSmEvent::SessionSpawned) => {
            SessionSmState::Live { count: count + 1 }
        }
        (SessionSmState::Live { count: 1 }, SessionSmEvent::SessionExited) => {
            SessionSmState::Draining { dialog_open: false }
        }
        (SessionSmState::Live { count }, SessionSmEvent::SessionExited) if count > 1 => {
            SessionSmState::Live { count: count - 1 }
        }
        (SessionSmState::Draining { .. }, SessionSmEvent::DialogOpened) => {
            SessionSmState::Draining { dialog_open: true }
        }
        (SessionSmState::Draining { dialog_open: true }, SessionSmEvent::DialogClosed) => {
            SessionSmState::Draining { dialog_open: false }
        }
        (SessionSmState::Draining { dialog_open: false }, SessionSmEvent::DrainCompleted) => {
            SessionSmState::Exited
        }
        // Defer drain while dialog open (INV-D19).
        (SessionSmState::Draining { dialog_open: true }, SessionSmEvent::DrainCompleted) => {
            SessionSmState::Draining { dialog_open: true }
        }
        (other, _) => other,
    }
}

#[cfg(test)]
#[path = "ports/tests.rs"]
mod tests;
