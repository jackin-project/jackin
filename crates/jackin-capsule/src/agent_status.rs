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

/// Scan raw PTY bytes for the first OSC 9;4 (`ConEmu` progress) state digit.
///
/// Sequence: `ESC ] 9 ; 4 ; <state>[;<pct>]` terminated by BEL or ST. Returns
/// the state digit (0 = clear/done-ish, 1/2/3 = active, 4 = paused). jackin-term
/// surfaces plain OSC 9 as a `Notification` passthrough but does not decode the
/// `9;4` progress sub-protocol, so it is scanned from the raw stream here —
/// the same model-independent approach as `scan_osc133`.
pub fn scan_osc9_progress(bytes: &[u8]) -> Option<u8> {
    const NEEDLE: &[u8] = b"\x1b]9;4;";
    let pos = bytes.windows(NEEDLE.len()).position(|w| w == NEEDLE)?;
    let digit = *bytes.get(pos + NEEDLE.len())?;
    digit.is_ascii_digit().then(|| digit - b'0')
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
        let next = self.effective_from_raw(raw);
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

    pub fn report(&self, detected_agent: Option<String>) -> AgentStatusReport {
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
