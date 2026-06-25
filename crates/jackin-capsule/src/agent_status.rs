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
//!   • Screen rule packs  (`rules`)       — structural terminal matching
//!   • OSC 133 markers    (`scan_osc133`)  — shell integration sequences
//!   • Hook/API reports   (`gating`)      — in-container reporter events
//!   • /proc process      (`process`)     — foreground process identity
//!
//!   evidence snapshot ───► arbitrate ───► debounce ───► SessionStatus
//! ```
//!
//! # Adding a new agent runtime
//!
//! 1. Add or extend `docker/runtime/agent-status/packs/<slug>.toml`.
//! 2. Add fixtures under `agent_status/screen/fixtures/<slug>/`.
//! 3. Add semantic event mapping in `gating.rs` only when the runtime ships
//!    hooks or a plugin surface.

pub mod arbitrate;
pub mod evidence;
pub mod gating;
pub mod hook_installer;
pub mod policy;
pub mod process;
pub mod rules;
pub mod seen;
pub mod sequence;

use evidence::{EvidenceSummary, RawAgentState};
use jackin_protocol::agent_status::{AgentStatusConfidence, AgentStatusReport, AgentStatusSource};

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

/// Per-session accumulated status. Holds the current effective state and
/// the `seen` flag used to derive `Done`.
#[derive(Debug, Clone)]
pub struct SessionStatus {
    /// Wire-format effective state consumed by the UI and protocol.
    pub effective: AgentState,
    /// Four-state raw status before `done` is derived from raw idle + unseen.
    pub raw: RawAgentState,
    /// Confidence of the evidence that produced `raw`.
    pub confidence: AgentStatusConfidence,
    /// Last evidence summary used to publish the current state.
    pub last_snapshot_summary: EvidenceSummary,
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
            raw: RawAgentState::Unknown,
            confidence: AgentStatusConfidence::Unknown,
            last_snapshot_summary: EvidenceSummary::default(),
            seen: true,
            revision: 0,
        }
    }

    pub fn publish_raw(
        &mut self,
        raw: RawAgentState,
        confidence: AgentStatusConfidence,
        mut summary: EvidenceSummary,
    ) -> Option<AgentState> {
        let previous = self.effective;
        let previous_raw = self.raw;
        let entering_work_cycle = matches!(raw, RawAgentState::Working | RawAgentState::Blocked)
            && !matches!(
                previous_raw,
                RawAgentState::Working | RawAgentState::Blocked
            );
        if entering_work_cycle {
            self.seen = false;
        }
        let next = self.effective_from_raw(raw, previous_raw);
        self.raw = raw;
        self.confidence = confidence;
        summary.raw_state = raw;
        summary.confidence = confidence;
        self.last_snapshot_summary = summary;
        if next == previous {
            None
        } else {
            self.effective = next;
            self.revision += 1;
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

    pub fn report(
        &self,
        detected_agent: Option<String>,
        last_seen_revision: u64,
    ) -> AgentStatusReport {
        let summary = &self.last_snapshot_summary;
        AgentStatusReport {
            raw_state: self.raw,
            source: summary.authority_source.as_ref().map_or_else(
                || match summary.winner {
                    evidence::EvidenceWinner::Authority => AgentStatusSource::None,
                    evidence::EvidenceWinner::Blocked | evidence::EvidenceWinner::Freeze => {
                        AgentStatusSource::VisibleScreen
                    }
                    evidence::EvidenceWinner::StrongVisualOrOsc => {
                        if summary.shell_integration {
                            AgentStatusSource::ShellIntegration
                        } else {
                            AgentStatusSource::VisibleScreen
                        }
                    }
                    evidence::EvidenceWinner::Physics => AgentStatusSource::ForegroundProcess,
                    evidence::EvidenceWinner::ProcessExit | evidence::EvidenceWinner::Unknown => {
                        AgentStatusSource::None
                    }
                },
                |source_id| AgentStatusSource::Reported {
                    source_id: source_id.clone(),
                },
            ),
            confidence: self.confidence,
            detected_agent,
            foreground_pgid: summary.foreground_pgid,
            visible_blocker: summary.visible_blocker,
            visible_idle: summary.visible_idle,
            visible_working: summary.visible_working,
            process_exited: summary.process_exited,
            foreground_returned_to_shell: summary.foreground_returned_to_shell,
            stale_report: summary.stale_report,
            subagents_active: summary.subagents_active,
            revision: self.revision,
            last_seen_revision,
        }
    }

    fn effective_from_raw(&self, raw: RawAgentState, _previous_raw: RawAgentState) -> AgentState {
        match raw {
            RawAgentState::Unknown => AgentState::Unknown,
            RawAgentState::Working => AgentState::Working,
            RawAgentState::Blocked => AgentState::Blocked,
            RawAgentState::Idle => {
                if self.seen {
                    AgentState::Idle
                } else {
                    AgentState::Done
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
        assert_eq!(s.raw, RawAgentState::Unknown);
        assert_eq!(s.revision, 0);
    }

    #[test]
    fn publish_working_transitions_unknown_to_working() {
        let mut s = SessionStatus::new();
        let changed = s.publish_raw(
            RawAgentState::Working,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        assert_eq!(changed, Some(AgentState::Working));
        assert_eq!(s.effective, AgentState::Working);
        assert_eq!(s.raw, RawAgentState::Working);
        assert!(!s.seen);
        assert_eq!(s.revision, 1);
    }

    #[test]
    fn idle_after_working_produces_done_when_unseen() {
        let mut s = SessionStatus::new();
        s.publish_raw(
            RawAgentState::Working,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        let changed = s.publish_raw(
            RawAgentState::Idle,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        assert_eq!(changed, Some(AgentState::Done));
        assert_eq!(s.effective, AgentState::Done);
    }

    #[test]
    fn repeated_idle_keeps_done_until_acknowledged() {
        let mut s = SessionStatus::new();
        s.publish_raw(
            RawAgentState::Working,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        s.publish_raw(
            RawAgentState::Idle,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        assert_eq!(s.effective, AgentState::Done);

        let changed = s.publish_raw(
            RawAgentState::Idle,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );

        assert_eq!(changed, None);
        assert_eq!(s.effective, AgentState::Done);
        assert!(!s.seen);
    }

    #[test]
    fn idle_after_working_produces_idle_when_seen() {
        let mut s = SessionStatus::new();
        s.publish_raw(
            RawAgentState::Working,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        s.seen = true;
        let changed = s.publish_raw(
            RawAgentState::Idle,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        assert_eq!(changed, Some(AgentState::Idle));
    }

    #[test]
    fn acknowledge_transitions_done_to_idle() {
        let mut s = SessionStatus::new();
        s.publish_raw(
            RawAgentState::Working,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        s.publish_raw(
            RawAgentState::Idle,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        assert_eq!(s.effective, AgentState::Done);
        let changed = s.acknowledge();
        assert_eq!(changed, Some(AgentState::Idle));
        assert_eq!(s.effective, AgentState::Idle);
        assert!(s.seen);
    }

    #[test]
    fn revision_increments_only_on_public_state_change() {
        let mut s = SessionStatus::new();
        assert_eq!(s.revision, 0);
        s.publish_raw(
            RawAgentState::Working,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        assert_eq!(s.revision, 1);
        s.publish_raw(
            RawAgentState::Working,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        assert_eq!(s.revision, 1);
        s.publish_raw(
            RawAgentState::Idle,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        assert_eq!(s.revision, 2);
    }

    #[test]
    fn blocked_enters_work_cycle_and_done_on_idle() {
        let mut s = SessionStatus::new();
        s.publish_raw(
            RawAgentState::Blocked,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        assert_eq!(s.effective, AgentState::Blocked);
        assert!(!s.seen);
        let changed = s.publish_raw(
            RawAgentState::Idle,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        assert_eq!(changed, Some(AgentState::Done));
    }

    #[test]
    fn re_work_after_ack_creates_new_done() {
        let mut s = SessionStatus::new();
        s.publish_raw(
            RawAgentState::Working,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        s.publish_raw(
            RawAgentState::Idle,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        assert_eq!(s.effective, AgentState::Done);
        s.acknowledge();
        assert_eq!(s.effective, AgentState::Idle);
        s.publish_raw(
            RawAgentState::Working,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        let changed = s.publish_raw(
            RawAgentState::Idle,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        assert_eq!(changed, Some(AgentState::Done));
    }

    #[test]
    fn publish_raw_keeps_latest_evidence_summary() {
        let mut s = SessionStatus::new();
        let summary = EvidenceSummary {
            rule_id: Some("claude.permission-dialog".to_owned()),
            visible_blocker: true,
            ..EvidenceSummary::default()
        };
        s.publish_raw(
            RawAgentState::Blocked,
            AgentStatusConfidence::Strong,
            summary,
        );
        assert_eq!(s.last_snapshot_summary.raw_state, RawAgentState::Blocked);
        assert_eq!(
            s.last_snapshot_summary.confidence,
            AgentStatusConfidence::Strong
        );
        assert_eq!(
            s.last_snapshot_summary.rule_id.as_deref(),
            Some("claude.permission-dialog")
        );
        assert!(s.last_snapshot_summary.visible_blocker);
    }

    #[test]
    fn report_uses_evidence_summary() {
        let mut s = SessionStatus::new();
        let summary = EvidenceSummary {
            authority_source: Some("hook-claude-1".to_owned()),
            foreground_pgid: Some(42),
            visible_working: true,
            subagents_active: 2,
            ..EvidenceSummary::default()
        };
        s.publish_raw(
            RawAgentState::Working,
            AgentStatusConfidence::Authoritative,
            summary,
        );
        let report = s.report(Some("claude".to_owned()), 0);
        assert_eq!(report.raw_state, RawAgentState::Working);
        assert_eq!(report.confidence, AgentStatusConfidence::Authoritative);
        assert_eq!(report.foreground_pgid, Some(42));
        assert!(report.visible_working);
        assert_eq!(report.subagents_active, 2);
        assert_eq!(
            report.source,
            AgentStatusSource::Reported {
                source_id: "hook-claude-1".to_owned()
            }
        );
    }

    #[test]
    fn report_preserves_shell_integration_source() {
        let mut s = SessionStatus::new();
        let summary = EvidenceSummary {
            winner: evidence::EvidenceWinner::StrongVisualOrOsc,
            shell_integration: true,
            ..EvidenceSummary::default()
        };
        s.publish_raw(RawAgentState::Idle, AgentStatusConfidence::Strong, summary);

        assert_eq!(
            s.report(Some("codex".to_owned()), 0).source,
            AgentStatusSource::ShellIntegration
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

    #[test]
    fn roll_up_priority_blocked_gt_done_gt_working_gt_idle_gt_unknown() {
        use crate::agent_status::arbitrate::attention_priority;
        assert!(attention_priority(AgentState::Blocked) > attention_priority(AgentState::Done));
        assert!(attention_priority(AgentState::Done) > attention_priority(AgentState::Working));
        assert!(attention_priority(AgentState::Working) > attention_priority(AgentState::Idle));
        assert!(attention_priority(AgentState::Idle) > attention_priority(AgentState::Unknown));
    }

    #[test]
    fn multiple_sessions_roll_up_reflects_most_urgent() {
        use crate::agent_status::arbitrate::roll_up_states;

        let session_states = vec![
            AgentState::Working,
            AgentState::Blocked,
            AgentState::Working,
            AgentState::Idle,
        ];
        let rolled = roll_up_states(&session_states);
        assert_eq!(rolled, AgentState::Blocked);
    }
}
