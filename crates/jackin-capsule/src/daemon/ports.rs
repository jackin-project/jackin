//! Daemon port seams at the real Multiplexer / session-supervisor boundary.
//!
//! Ports own effectful decisions (attach displace, runtime-event ACK,
//! codename retirement, last-session exit deferral) rather than taking
//! pre-computed booleans alone. Production uses [`DefaultDaemonPorts`];
//! tests drive [`FakeDaemonPorts`] for observable attach/displace behavior.
//!
//! ## Sim / state-machine tooling evaluation (plan 017 step 4)
//!
//! | Tool | Verdict | Rationale |
//! |---|---|---|
//! | `proptest-state-machine` | **defer** | Fixed FakeDaemonPorts + daemon suite already cover attach/displace/reattach transitions; a state-machine suite would restate those INV paths without a measured gap. Revisit if SessionSupervisor gains more async edges. |
//! | `turmoil` / `madsim` | **defer** | No network-partition surface inside the in-container daemon event loop; host↔capsule networking is integration-tested elsewhere. |
//! | `fail` / failpoints | **defer** | PTY failure is already injectable via FakeDaemonPorts::mark_pty_failure; process-wide failpoints add CI flakiness without new coverage. |

#[cfg(test)]
use std::sync::Mutex;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Control-channel decisions for one-shot status / runtime-event replies.
pub(crate) trait ControlPort {
    /// Whether an unknown-session runtime event still receives an ACK
    /// (agent hooks must never block; unknown session is logged, not failed).
    fn should_ack_unknown_session_runtime_event(
        &self,
        session_id: u64,
        session_known: bool,
    ) -> bool;
}

/// Attach lifecycle: single active client; Hello displaces the previous client.
pub(crate) trait AttachPort {
    /// True when an incoming Hello must displace an already-attached client.
    ///
    /// `has_active_client` is the live attach-registry observation at the
    /// Hello site; ports may also consult their own attach ledger.
    fn should_displace_on_hello(&self, has_active_client: bool) -> bool;

    /// Record that a client attached (after displace decision applied).
    fn record_attach(&self) {}

    /// Record that the active client detached.
    fn record_detach(&self) {}
}

/// Status publication surface (tab labels, session list).
pub(crate) trait StatusPort {
    /// True when removing an exited session should retire its codename.
    fn should_retire_codename_on_exit(
        &self,
        session_id: u64,
        remaining_live_sessions: usize,
    ) -> bool;
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
    fn should_ack_unknown_session_runtime_event(
        &self,
        _session_id: u64,
        _session_known: bool,
    ) -> bool {
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
    fn should_retire_codename_on_exit(
        &self,
        _session_id: u64,
        _remaining_live_sessions: usize,
    ) -> bool {
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

/// Test double that records attach/displace/detach and can force PTY-style
/// failure flags for boundary harnesses.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct FakeDaemonPorts {
    /// When true, `should_displace_on_hello` always returns true (even without
    /// an active client) — simulates a sticky displace policy.
    pub force_displace: AtomicBool,
    /// When true, control ACKs for unknown sessions are refused (negative path).
    pub refuse_unknown_ack: AtomicBool,
    /// When true, codename retirement is skipped.
    pub skip_codename_retire: AtomicBool,
    /// When true, last-session exit is always deferred.
    pub force_defer_exit: AtomicBool,
    /// Attach count ledger.
    pub attach_count: AtomicUsize,
    /// Detach count ledger.
    pub detach_count: AtomicUsize,
    /// Displace decisions observed (`has_active_client` inputs).
    pub displace_observations: Mutex<Vec<bool>>,
    /// Simulated PTY failure sticky flag for harnesses.
    pub pty_failure: AtomicBool,
}

#[cfg(test)]
impl FakeDaemonPorts {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn mark_pty_failure(&self) {
        self.pty_failure.store(true, Ordering::SeqCst);
    }

    pub(crate) fn pty_failed(&self) -> bool {
        self.pty_failure.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
impl ControlPort for FakeDaemonPorts {
    fn should_ack_unknown_session_runtime_event(
        &self,
        _session_id: u64,
        session_known: bool,
    ) -> bool {
        if self.refuse_unknown_ack.load(Ordering::SeqCst) && !session_known {
            return false;
        }
        true
    }
}

#[cfg(test)]
impl AttachPort for FakeDaemonPorts {
    fn should_displace_on_hello(&self, has_active_client: bool) -> bool {
        if let Ok(mut obs) = self.displace_observations.lock() {
            obs.push(has_active_client);
        }
        if self.force_displace.load(Ordering::SeqCst) {
            return true;
        }
        has_active_client
    }

    fn record_attach(&self) {
        self.attach_count.fetch_add(1, Ordering::SeqCst);
    }

    fn record_detach(&self) {
        self.detach_count.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
impl StatusPort for FakeDaemonPorts {
    fn should_retire_codename_on_exit(
        &self,
        _session_id: u64,
        _remaining_live_sessions: usize,
    ) -> bool {
        !self.skip_codename_retire.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
impl PersistencePort for FakeDaemonPorts {
    fn defer_last_session_exit(&self, dialog_open: bool) -> bool {
        if self.force_defer_exit.load(Ordering::SeqCst) {
            return true;
        }
        super::should_defer_last_session_exit(dialog_open)
    }
}

#[cfg(test)]
mod tests;
