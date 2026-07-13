//! Daemon port seams for control, attach, status, and persistence.
//!
//! Production code calls these pure decision helpers at the real Multiplexer
//! boundaries (control replies, Hello takeover, codename retirement, last-
//! session exit). Characterization tests exercise those call sites.

/// Control-channel decisions for one-shot status / runtime-event replies.
pub(crate) trait ControlPort {
    /// Whether an unknown session still receives an ACK (agent hooks must
    /// never block; unknown session is logged, not failed).
    fn should_ack_unknown_session_runtime_event(&self, session_known: bool) -> bool;
}

/// Attach lifecycle: single active client; Hello displaces the previous client.
pub(crate) trait AttachPort {
    /// True when an incoming Hello must displace an already-attached client.
    fn should_displace_on_hello(&self, has_active_client: bool) -> bool;
}

/// Status publication surface (tab labels, session list).
pub(crate) trait StatusPort {
    /// True when removing an exited session should retire its codename.
    fn should_retire_codename_on_exit(&self, remaining_live_sessions: usize) -> bool;
}

/// Persistence / reattach decisions after last-session exit.
pub(crate) trait PersistencePort {
    /// True when last-session exit handling should be deferred (dialog open).
    fn defer_last_session_exit(&self, dialog_open: bool) -> bool;
}

/// Default production-shaped port implementations (INV rules).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DefaultDaemonPorts;

impl ControlPort for DefaultDaemonPorts {
    fn should_ack_unknown_session_runtime_event(&self, _session_known: bool) -> bool {
        // INV-D12: always ACK so agent hooks never block on control.
        true
    }
}

impl AttachPort for DefaultDaemonPorts {
    fn should_displace_on_hello(&self, has_active_client: bool) -> bool {
        // INV-D1: at most one attach client; Hello displaces when one is live.
        has_active_client
    }
}

impl StatusPort for DefaultDaemonPorts {
    fn should_retire_codename_on_exit(&self, _remaining_live_sessions: usize) -> bool {
        // INV-D8: always retire the exited session's codename so labels update.
        true
    }
}

impl PersistencePort for DefaultDaemonPorts {
    fn defer_last_session_exit(&self, dialog_open: bool) -> bool {
        super::should_defer_last_session_exit(dialog_open)
    }
}

/// Production ports singleton used at call sites.
pub(crate) const PORTS: DefaultDaemonPorts = DefaultDaemonPorts;

#[cfg(test)]
mod tests;
