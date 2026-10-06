//! jackin-agent-status: agent-status packs, hooks, and rule evaluation.
//!
//! **Architecture Invariant:** T2.
//! Entry point: [`evaluate_rules`] — status pack rule evaluation.

pub mod arbitrate;
pub mod evidence;
pub mod gating;
pub mod policy;
pub mod process;
pub mod rules;

use evidence::{EvidenceSummary, RawAgentState};
use jackin_protocol::agent_status::{AgentStatusConfidence, AgentStatusReport, AgentStatusSource};
use jackin_protocol::control::AgentState;

mod osc;
pub use osc::{OscShellMark, OscStatusDecoder, OscStatusEvent};

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

    pub fn publish_raw(&mut self, summary: EvidenceSummary) -> Option<AgentState> {
        let raw = summary.raw_state;
        let confidence = summary.confidence;
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
        let next = self.effective_from_raw(raw);
        // `raw`/`confidence` were read from `summary` above, so the cached copies
        // on self are exactly the incoming summary's verdict — kept for `report()`
        // and the next tick's `previous_raw` without re-reading the summary.
        self.raw = raw;
        self.confidence = confidence;
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

    pub fn report(&self, detected_agent: Option<String>) -> AgentStatusReport {
        let summary = &self.last_snapshot_summary;
        AgentStatusReport {
            raw_state: self.raw,
            // The reported source is a pure function of the winning channel — no
            // separate flag to fall out of sync with the winner.
            source: match &summary.winner {
                evidence::EvidenceWinner::Authority { source_id } => AgentStatusSource::Reported {
                    source_id: source_id.clone(),
                },
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
        }
    }

    fn effective_from_raw(&self, raw: RawAgentState) -> AgentState {
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
mod tests;
