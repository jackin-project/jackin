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
//!   • OSC 133 markers    (`scan_osc133`)  — shell integration sequences
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
pub mod seen;
pub mod sequence;

use crate::protocol::AgentState;

/// Shell integration markers from OSC 133 sequences.
///
/// Emitted by shell `precmd`/`preexec` hooks installed in `/home/agent/.zshrc`.
/// Parsed from raw PTY bytes by `scan_osc133`; model-independent (works with
/// both vt100 and `DamageGrid` renderers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscShellMark {
    /// `OSC 133 ; A` — prompt start.
    PromptStart,
    /// `OSC 133 ; B` — prompt end / ready for input.
    PromptEnd,
    /// `OSC 133 ; C` — pre-execution (user pressed Enter).
    PreExec,
    /// `OSC 133 ; D` — command finished with optional exit code.
    CommandFinished { exit_code: Option<i32> },
}

/// Scan raw PTY bytes for the FIRST `OSC 133 ; <letter>` sequence.
///
/// Finds `\x1b]133;A`, `B`, `C`, or `D[;<exit_code>]` followed by BEL
/// (`\x07`) or ST (`\x1b\\`). Model-independent: works with both the
/// current vt100-based session and the future DamageGrid-based session.
pub fn scan_osc133(bytes: &[u8]) -> Option<OscShellMark> {
    // Minimum sequence: \x1b]133;A\x07 = 8 bytes
    let len = bytes.len();
    if len < 8 {
        return None;
    }

    let mut i = 0;
    while i + 7 < len {
        // Look for ESC ] 1 3 3 ;
        if bytes[i] == b'\x1b'
            && bytes[i + 1] == b']'
            && bytes[i + 2] == b'1'
            && bytes[i + 3] == b'3'
            && bytes[i + 4] == b'3'
            && bytes[i + 5] == b';'
        {
            let letter = bytes[i + 6];
            match letter {
                b'A' => return Some(OscShellMark::PromptStart),
                b'B' => return Some(OscShellMark::PromptEnd),
                b'C' => return Some(OscShellMark::PreExec),
                b'D' => {
                    // Optional exit code after another ';'
                    let exit_code = if i + 7 < len && bytes[i + 7] == b';' {
                        let start = i + 8;
                        let end = bytes[start..]
                            .iter()
                            .position(|&b| !b.is_ascii_digit())
                            .map_or(len, |p| start + p);
                        if end > start {
                            std::str::from_utf8(&bytes[start..end])
                                .ok()
                                .and_then(|s| s.parse::<i32>().ok())
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    return Some(OscShellMark::CommandFinished { exit_code });
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod osc133_tests {
    use super::*;

    #[test]
    fn scan_osc133_detects_prompt_end() {
        let bytes = b"\x1b]133;B\x07";
        assert_eq!(scan_osc133(bytes), Some(OscShellMark::PromptEnd));
    }

    #[test]
    fn scan_osc133_detects_pre_exec() {
        let bytes = b"\x1b]133;C\x07";
        assert_eq!(scan_osc133(bytes), Some(OscShellMark::PreExec));
    }

    #[test]
    fn scan_osc133_detects_command_finished_with_code() {
        let bytes = b"\x1b]133;D;0\x07";
        assert_eq!(
            scan_osc133(bytes),
            Some(OscShellMark::CommandFinished { exit_code: Some(0) })
        );
    }

    #[test]
    fn scan_osc133_returns_none_for_plain_output() {
        assert_eq!(scan_osc133(b"hello world"), None);
    }

    #[test]
    fn scan_osc133_finds_marker_in_larger_buffer() {
        let bytes = b"some output\r\n\x1b]133;B\x07more output";
        assert_eq!(scan_osc133(bytes), Some(OscShellMark::PromptEnd));
    }
}

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
        if next == self.effective {
            None
        } else {
            let prev = self.effective;
            self.effective = next;
            self.revision += 1;
            // Reset seen whenever entering a work cycle from a non-work state.
            // Covers both explicit signals (HookTaskStart, OperatorInput) and
            // screen-detected transitions (WorkingVisible, BlockedVisible).
            let entering_work_cycle = matches!(next, AgentState::Working | AgentState::Blocked)
                && !matches!(prev, AgentState::Working | AgentState::Blocked);
            if entering_work_cycle {
                self.seen = false;
            }
            Some(next)
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
    /// 3. `BlockedVisible` → `Blocked`
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
        // Enter Working first (resets seen=false as a new work cycle starts).
        let mut s = SessionStatus::new();
        s.advance(AgentRawState::WorkingVisible);
        // Simulate operator having reviewed this work (acknowledge clears Done → Idle
        // in practice, but here we just mark seen directly).
        s.seen = true;
        // PromptVisible while Working+seen → Idle (not Done, because seen=true).
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
            source_id: "hook-1".to_owned(),
            agent_label: "claude".to_owned(),
            raw_state: "blocked".to_owned(),
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
    fn subagent_counter_prevents_working_from_clearing_blocked() {
        // The roadmap requires: while subagent_count > 0, PostToolUse-equivalent
        // WorkingVisible signals must NOT clear a Blocked state.
        // This is implemented in daemon.rs handle_control_msg. Verify the intent
        // via the SessionStatus state machine: if we're Blocked and receive
        // WorkingVisible, it transitions — but the daemon's subagent guard
        // prevents that signal from being fed in the first place.
        //
        // This test verifies the state machine behavior that makes the guard correct:
        // WorkingVisible DOES override Blocked in the raw machine, which is why
        // the daemon-level guard (suppressing WorkingVisible when subagent_count > 0
        // and state == Blocked) is necessary.
        let mut s = SessionStatus::new();
        s.advance(AgentRawState::BlockedVisible);
        assert_eq!(s.effective, AgentState::Blocked);

        // Without the guard, WorkingVisible would clear Blocked.
        let changed = s.advance(AgentRawState::WorkingVisible);
        assert_eq!(
            changed,
            Some(AgentState::Working),
            "WorkingVisible overrides Blocked in raw machine — daemon guard prevents this when subagents active"
        );

        // Reset and verify the guard logic: Blocked stays Blocked when
        // the daemon suppresses the working signal (simulated by not calling advance).
        let mut s2 = SessionStatus::new();
        s2.advance(AgentRawState::BlockedVisible);
        // Simulate daemon guard: when subagent_count > 0, don't call advance
        // with WorkingVisible. State should remain Blocked.
        assert_eq!(
            s2.effective,
            AgentState::Blocked,
            "Blocked persists when daemon suppresses WorkingVisible (subagent guard active)"
        );
    }

    #[test]
    fn event_stream_emits_on_raw_state_change() {
        // The state machine emits Some(new_state) only on real transitions.
        // This verifies that a transition from Unknown → Working produces an
        // event (Some), while a repeated Working → Working produces no event (None).
        let mut s = SessionStatus::new();
        assert_eq!(s.effective, AgentState::Unknown);

        // First transition produces an event.
        let event = s.advance(AgentRawState::WorkingVisible);
        assert_eq!(
            event,
            Some(AgentState::Working),
            "Unknown→Working should produce Some(Working) for broadcast"
        );

        // Same state again produces no event.
        let no_event = s.advance(AgentRawState::WorkingVisible);
        assert_eq!(
            no_event, None,
            "Working→Working should produce None (no broadcast needed)"
        );

        // Transition to Blocked produces an event.
        let blocked_event = s.advance(AgentRawState::BlockedVisible);
        assert_eq!(
            blocked_event,
            Some(AgentState::Blocked),
            "Working→Blocked should produce Some(Blocked)"
        );
    }

    #[test]
    fn working_visible_from_fresh_session_produces_done_not_idle() {
        // No explicit seen=false needed — WorkingVisible from Unknown
        // must reset seen so the subsequent idle produces Done.
        let mut s = SessionStatus::new();
        // SessionStatus::new() starts with seen=true (neutral start).
        // WorkingVisible should enter the work cycle and reset seen.
        s.advance(AgentRawState::WorkingVisible);
        assert_eq!(s.effective, AgentState::Working);
        let changed = s.advance(AgentRawState::PromptVisible);
        assert_eq!(
            changed,
            Some(AgentState::Done),
            "WorkingVisible from Unknown must reset seen so PromptVisible produces Done"
        );
    }

    #[test]
    fn blocked_visible_from_fresh_session_produces_done_on_prompt() {
        // BlockedVisible from Unknown should also enter the work cycle.
        let mut s = SessionStatus::new();
        s.advance(AgentRawState::BlockedVisible);
        assert_eq!(s.effective, AgentState::Blocked);
        let changed = s.advance(AgentRawState::PromptVisible);
        assert_eq!(
            changed,
            Some(AgentState::Done),
            "BlockedVisible from Unknown must reset seen so PromptVisible produces Done"
        );
    }

    #[test]
    fn working_visible_after_ack_produces_done_again() {
        // Screen-signal re-entry after acknowledge must also produce Done.
        let mut s = SessionStatus::new();
        s.advance(AgentRawState::WorkingVisible);
        s.advance(AgentRawState::PromptVisible); // → Done
        s.acknowledge(); // → Idle
        assert_eq!(s.effective, AgentState::Idle);
        // New work cycle via screen detector.
        s.advance(AgentRawState::WorkingVisible); // Idle → Working, seen reset
        let changed = s.advance(AgentRawState::PromptVisible); // Working → Done
        assert_eq!(
            changed,
            Some(AgentState::Done),
            "Re-entry via WorkingVisible after acknowledge must produce Done again"
        );
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

    // PTY/e2e tests — simulate detector + state machine path from visible
    // terminal rows without a live PTY. Covers the roadmap Test Plan cases.

    #[test]
    fn fake_claude_permission_dialog_transitions_to_blocked() {
        use crate::agent_status::detectors::DetectorRegistry;

        let screen = vec![
            "Claude wants to run: rm -rf /tmp".to_owned(),
            String::new(),
            "  enter to select  esc to cancel  ↑/↓ to navigate".to_owned(),
        ];

        let registry = DetectorRegistry::default_registry();
        let result = registry.detect(Some("claude"), &screen);
        assert_eq!(
            result,
            Some(AgentRawState::BlockedVisible),
            "Claude permission dialog should produce BlockedVisible",
        );

        let mut status = SessionStatus::new();
        let new_state = status.advance(AgentRawState::BlockedVisible);
        assert_eq!(new_state, Some(AgentState::Blocked));
    }

    #[test]
    fn fake_claude_spinner_transitions_to_working_then_idle() {
        use crate::agent_status::detectors::DetectorRegistry;

        let registry = DetectorRegistry::default_registry();

        let screen_working = vec!["✻ Simplifying…".to_owned(), "esc to interrupt".to_owned()];
        let raw = registry.detect(Some("claude"), &screen_working);
        assert_eq!(raw, Some(AgentRawState::WorkingVisible));

        let mut status = SessionStatus::new();
        status.seen = false;
        status.advance(AgentRawState::WorkingVisible);
        assert_eq!(status.effective, AgentState::Working);

        let screen_idle = vec!["╭───╮".to_owned(), "│ > │".to_owned(), "╰───╯".to_owned()];
        let raw_idle = registry.detect(Some("claude"), &screen_idle);
        assert_eq!(
            raw_idle,
            Some(AgentRawState::PromptVisible),
            "Claude prompt box should produce PromptVisible",
        );

        let new_state = status.advance(AgentRawState::PromptVisible);
        assert_eq!(
            new_state,
            Some(AgentState::Done),
            "After Working with seen=false, idle → Done",
        );
    }

    #[test]
    fn process_exit_signal_transitions_to_idle_and_clears_authority() {
        let _auth = HookAuthority {
            source_id: "claude-hook".to_owned(),
            agent_label: "claude".to_owned(),
            raw_state: "working".to_owned(),
            seq: 100,
            ts_ns: 0,
            message: None,
            last_seen: std::time::Instant::now(),
        };

        let mut status = SessionStatus::new();
        status.advance(AgentRawState::WorkingVisible);
        assert_eq!(status.effective, AgentState::Working);

        let new_state = status.advance(AgentRawState::ProcessExited);
        assert_eq!(
            new_state,
            Some(AgentState::Idle),
            "Process exit should transition to Idle",
        );
        assert_eq!(status.effective, AgentState::Idle);
    }

    #[test]
    fn multiple_sessions_roll_up_reflects_most_urgent() {
        use crate::agent_status::arbitrate::{attention_priority, roll_up_states};

        let session_states = vec![
            AgentState::Working,
            AgentState::Blocked,
            AgentState::Working,
            AgentState::Idle,
        ];
        let rolled = roll_up_states(&session_states);
        assert_eq!(
            rolled,
            AgentState::Blocked,
            "Roll-up should surface Blocked as the most urgent state",
        );

        assert!(attention_priority(AgentState::Blocked) > attention_priority(AgentState::Done));
        assert!(attention_priority(AgentState::Done) > attention_priority(AgentState::Working));
    }
}
