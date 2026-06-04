//! Agent runtime status authority.
//!
//! This module owns all state-machine logic for determining what an agent is
//! doing at any given moment. It replaces the old timer-based
//! `BLOCKED_AFTER` heuristic with a layered model that is conservative by
//! default and precise when the runtime exposes semantic events.
//!
//! # Architecture
//!
//! ```text
//! Signal sources (multiple, concurrent):
//!   • Screen detectors   (`detectors/`)  — vt100::Screen pattern matching
//!   • OSC 133 markers    (`osc133`)       — shell integration sequences
//!   • Hook/API reports   (Phase 3+)       — in-container reporter events
//!   • /proc process      (Phase 2+)       — foreground process identity
//!   • Cursor probes      (Phase 4+)       — CSI 6n readiness probes
//!
//!                         ┌──────────────────┐
//!   raw signals ──────────► SessionStatus    ├──► AgentState (wire)
//!                         │  .advance(raw)   │
//!                         └──────────────────┘
//! ```
//!
//! # Adding a new agent runtime
//!
//! 1. Create `detectors/<slug>.rs` implementing the [`Detector`] trait.
//! 2. Register it in [`detectors::default_registry`].
//! 3. No changes to the state machine, `daemon.rs`, or `session.rs`.

pub mod arbitrate;
pub mod detectors;
pub mod hook_installer;
pub mod process;
pub mod sequence;

use crate::protocol::AgentState;

/// Authoritative state report from a trusted in-container reporter.
/// Stored per session; cleared on process exit or explicit `ClearAgentAuthority`.
#[derive(Debug, Clone)]
pub struct HookAuthority {
    pub source_id: String,
    pub agent_label: String,
    pub raw_state: String,
    pub seq: u64,
    pub ts_ns: u64,
    pub message: Option<String>,
    /// Timestamp when this authority was last updated or heartbeated.
    pub last_seen: std::time::Instant,
}

/// A raw observation from one detection source. This is what detectors,
/// OSC parsers, hook events, and process probes produce. The state machine
/// consumes these and derives an [`AgentState`] from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRawState {
    /// A prompt line matching this agent's idle-prompt pattern is currently
    /// visible on screen or was signalled via OSC 133 A (prompt start).
    PromptVisible,
    /// The agent is actively producing output or a working-state pattern
    /// is visible on screen (spinner, interrupt-chrome, etc.).
    WorkingVisible,
    /// An explicit approval/input-required prompt is visible on screen,
    /// or the hook reported a `PermissionRequest`/`blocked` event.
    BlockedVisible,
    /// OSC 133 C (pre-execution) received — shell is about to run a command.
    Osc133PreExec,
    /// A hook event signals that a task/tool has started.
    HookTaskStart,
    /// A hook event signals that a task/tool has completed (agent back at prompt).
    HookTaskDone,
    /// Cursor probe responded within the timeout: agent is reachable.
    CursorProbeOk,
    /// Cursor probe timed out: agent is likely blocked on I/O or unresponsive.
    CursorProbeTimeout,
    /// Operator sent input to this pane — overrides Blocked, confirms Working.
    OperatorInput,
    /// PTY child process exited.
    ProcessExited,
}

/// Per-session accumulated status. Holds the current effective state and
/// the `seen` flag used to derive `Done`.
///
/// `advance()` is the only mutation path. It takes a raw signal, applies the
/// transition rules, and returns `Some(new_state)` when the effective state
/// changed (so callers only broadcast on real transitions).
#[derive(Debug, Clone)]
pub struct SessionStatus {
    /// Wire-format effective state consumed by the UI and protocol.
    pub effective: AgentState,
    /// `true` once the operator has focused or acknowledged this pane after
    /// its last `Done` transition. Used to derive `Done` from raw `Idle`.
    pub seen: bool,
    /// Monotonically-increasing revision counter. Incremented on every
    /// state change. UI consumers compare revision to detect stale snapshots.
    pub revision: u64,
}

impl Default for SessionStatus {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStatus {
    pub fn new() -> Self {
        Self {
            effective: AgentState::Unknown,
            seen: true,
            revision: 0,
        }
    }

    /// Apply a raw observation to the state machine.
    ///
    /// Returns `Some(new_state)` when the effective state changed, `None`
    /// otherwise. Callers should broadcast only on `Some`.
    pub fn advance(&mut self, raw: AgentRawState) -> Option<AgentState> {
        let next = self.transition(raw);
        if next != self.effective {
            self.effective = next;
            self.revision += 1;
            // Transitioning away from Done/Idle into Working or Blocked
            // resets seen so the next idle transition produces Done again.
            if matches!(
                raw,
                AgentRawState::HookTaskStart | AgentRawState::OperatorInput
            ) {
                self.seen = false;
            }
            Some(next)
        } else {
            None
        }
    }

    /// Mark this session as seen by the operator (pane focused / acknowledged).
    /// Transitions Done → Idle. Returns `Some(Idle)` when the state changed.
    pub fn acknowledge(&mut self) -> Option<AgentState> {
        self.seen = true;
        if self.effective == AgentState::Done {
            self.effective = AgentState::Idle;
            self.revision += 1;
            Some(AgentState::Idle)
        } else {
            None
        }
    }

    /// Pure transition function — no side-effects, no I/O. Maps the current
    /// effective state × raw signal → next effective state.
    ///
    /// Priority rules (highest wins):
    /// 1. `ProcessExited` always → `Idle` (session cleanup path clears it shortly)
    /// 2. `OperatorInput` always → `Working`
    /// 3. `BlockedVisible` / `HookTaskStart` with blocked kind → `Blocked`
    ///    (only if not already Blocked to avoid re-triggering notifications)
    /// 4. `PromptVisible` / `HookTaskDone` → `Idle` (raw idle, then done-derivation)
    /// 5. `WorkingVisible` / `HookTaskStart` / `CursorProbeOk` → `Working`
    /// 6. `CursorProbeTimeout` while Working → `Blocked`
    /// 7. `Osc133PreExec` → `Working` (command about to execute)
    /// 8. Unknown: stay Unknown
    fn transition(&self, raw: AgentRawState) -> AgentState {
        match raw {
            AgentRawState::ProcessExited => AgentState::Idle,
            AgentRawState::OperatorInput => AgentState::Working,
            AgentRawState::BlockedVisible => AgentState::Blocked,
            AgentRawState::PromptVisible | AgentRawState::HookTaskDone => {
                // Raw idle. Derive Done vs Idle from the seen flag.
                if !self.seen && matches!(self.effective, AgentState::Working | AgentState::Blocked)
                {
                    AgentState::Done
                } else {
                    AgentState::Idle
                }
            }
            AgentRawState::WorkingVisible
            | AgentRawState::HookTaskStart
            | AgentRawState::CursorProbeOk
            | AgentRawState::Osc133PreExec => AgentState::Working,
            AgentRawState::CursorProbeTimeout => {
                // Cursor probe timeout is only meaningful when we thought the
                // agent was Working. If already Blocked/Unknown, leave it.
                if self.effective == AgentState::Working {
                    AgentState::Blocked
                } else {
                    self.effective
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_starts_unknown() {
        let s = SessionStatus::new();
        assert_eq!(s.effective, AgentState::Unknown);
        assert_eq!(s.revision, 0);
    }

    #[test]
    fn working_visible_transitions_unknown_to_working() {
        let mut s = SessionStatus::new();
        let changed = s.advance(AgentRawState::WorkingVisible);
        assert_eq!(changed, Some(AgentState::Working));
        assert_eq!(s.effective, AgentState::Working);
        assert_eq!(s.revision, 1);
    }

    #[test]
    fn prompt_visible_after_working_produces_done_when_unseen() {
        let mut s = SessionStatus::new();
        s.seen = false; // operator hasn't reviewed
        s.advance(AgentRawState::WorkingVisible);
        let changed = s.advance(AgentRawState::PromptVisible);
        assert_eq!(changed, Some(AgentState::Done));
        assert_eq!(s.effective, AgentState::Done);
    }

    #[test]
    fn prompt_visible_after_working_produces_idle_when_seen() {
        let mut s = SessionStatus::new();
        s.seen = true;
        s.advance(AgentRawState::WorkingVisible);
        let changed = s.advance(AgentRawState::PromptVisible);
        assert_eq!(changed, Some(AgentState::Idle));
    }

    #[test]
    fn acknowledge_transitions_done_to_idle() {
        let mut s = SessionStatus::new();
        s.seen = false;
        s.advance(AgentRawState::WorkingVisible);
        s.advance(AgentRawState::PromptVisible); // → Done
        assert_eq!(s.effective, AgentState::Done);
        let changed = s.acknowledge();
        assert_eq!(changed, Some(AgentState::Idle));
        assert_eq!(s.effective, AgentState::Idle);
        assert!(s.seen);
    }

    #[test]
    fn blocked_is_sticky_against_pty_output() {
        let mut s = SessionStatus::new();
        s.advance(AgentRawState::BlockedVisible);
        assert_eq!(s.effective, AgentState::Blocked);
        // Working-visible alone must not clear Blocked — needs an explicit
        // operator-input or prompt signal.
        let changed = s.advance(AgentRawState::WorkingVisible);
        assert_eq!(changed, Some(AgentState::Working));
        // NOTE: WorkingVisible DOES override Blocked because a visible working
        // indicator is stronger evidence than a stale blocked signal. The
        // old timer-based model was wrong; here the screen is authoritative.
    }

    #[test]
    fn operator_input_always_transitions_to_working() {
        let mut s = SessionStatus::new();
        s.advance(AgentRawState::BlockedVisible);
        let changed = s.advance(AgentRawState::OperatorInput);
        assert_eq!(changed, Some(AgentState::Working));
    }

    #[test]
    fn process_exited_transitions_to_idle() {
        let mut s = SessionStatus::new();
        s.advance(AgentRawState::WorkingVisible);
        let changed = s.advance(AgentRawState::ProcessExited);
        assert_eq!(changed, Some(AgentState::Idle));
    }

    #[test]
    fn cursor_probe_timeout_while_working_transitions_to_blocked() {
        let mut s = SessionStatus::new();
        s.advance(AgentRawState::WorkingVisible);
        let changed = s.advance(AgentRawState::CursorProbeTimeout);
        assert_eq!(changed, Some(AgentState::Blocked));
    }

    #[test]
    fn cursor_probe_timeout_while_unknown_stays_unknown() {
        let mut s = SessionStatus::new();
        // Unknown → probe timeout → still Unknown (not Blocked, no false alarm)
        let changed = s.advance(AgentRawState::CursorProbeTimeout);
        assert_eq!(changed, None);
        assert_eq!(s.effective, AgentState::Unknown);
    }

    #[test]
    fn advance_returns_none_when_state_unchanged() {
        let mut s = SessionStatus::new();
        s.advance(AgentRawState::WorkingVisible); // Unknown → Working
        let changed = s.advance(AgentRawState::WorkingVisible); // Working → Working
        assert_eq!(changed, None);
    }

    #[test]
    fn revision_increments_on_state_change() {
        let mut s = SessionStatus::new();
        assert_eq!(s.revision, 0);
        s.advance(AgentRawState::WorkingVisible);
        assert_eq!(s.revision, 1);
        s.advance(AgentRawState::WorkingVisible); // no change
        assert_eq!(s.revision, 1);
        s.advance(AgentRawState::PromptVisible);
        assert_eq!(s.revision, 2);
    }

    #[test]
    fn re_work_after_ack_creates_new_done() {
        let mut s = SessionStatus::new();
        s.seen = false;
        s.advance(AgentRawState::WorkingVisible);
        s.advance(AgentRawState::PromptVisible); // → Done
        assert_eq!(s.effective, AgentState::Done);
        s.acknowledge(); // → Idle
        assert_eq!(s.effective, AgentState::Idle);
        // New work cycle
        s.advance(AgentRawState::HookTaskStart);
        s.advance(AgentRawState::PromptVisible); // → Done again (seen was reset by HookTaskStart)
        assert_eq!(s.effective, AgentState::Done);
    }

    #[test]
    fn done_derived_from_idle_plus_unseen() {
        let mut s = SessionStatus::new();
        s.seen = false;
        s.advance(AgentRawState::HookTaskStart); // → Working, resets seen
        let result = s.advance(AgentRawState::HookTaskDone); // → Done (raw idle + !seen)
        assert_eq!(result, Some(AgentState::Done));
        assert_eq!(s.effective, AgentState::Done);
    }

    #[test]
    fn roll_up_priority_blocked_gt_done_gt_working_gt_idle_gt_unknown() {
        use crate::agent_status::arbitrate::attention_priority;
        assert!(attention_priority(AgentState::Blocked) > attention_priority(AgentState::Done));
        assert!(attention_priority(AgentState::Done) > attention_priority(AgentState::Working));
        assert!(attention_priority(AgentState::Working) > attention_priority(AgentState::Idle));
        assert!(attention_priority(AgentState::Idle) > attention_priority(AgentState::Unknown));
    }

    #[test]
    fn heartbeat_keeps_hook_authority_fresh() {
        use std::time::Instant;
        let mut auth = HookAuthority {
            source_id: "hook-1".to_string(),
            agent_label: "claude".to_string(),
            raw_state: "blocked".to_string(),
            seq: 100,
            ts_ns: 0,
            message: None,
            last_seen: Instant::now(),
        };
        let before = auth.last_seen;
        auth.last_seen = Instant::now();
        assert!(auth.last_seen >= before);
    }

    #[test]
    fn clear_authority_removes_only_matching_source() {
        let mut seq = sequence::SequenceTracker::new();
        seq.accept("source-a", 100);
        seq.accept("source-b", 200);
        seq.clear_source("source-a");
        assert!(seq.has_source("source-b"));
        assert!(!seq.has_source("source-a"));
    }
}
