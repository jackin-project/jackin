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

#[derive(Debug, Clone)]
pub struct SessionStatus {
    pub effective: AgentState,

    pub raw: RawAgentState,

    pub confidence: AgentStatusConfidence,

    pub last_snapshot_summary: EvidenceSummary,

    pub seen: bool,

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
