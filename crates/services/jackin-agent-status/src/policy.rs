// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, Instant};

use jackin_protocol::agent_status::AgentStatusConfidence;

use crate::evidence::{EvidenceNote, EvidenceSummary, EvidenceWinner, RawAgentState};
use jackin_protocol::control::AgentState;

pub const AUTHORITY_TTL: Duration = Duration::from_secs(30);
pub const OSC_SHELL_TTL: Duration = AUTHORITY_TTL;
pub const WATCHDOG_QUIET: Duration = Duration::from_secs(10);
pub const IDLE_CONFIRMATIONS: u8 = 3;
pub const IDLE_HOLD_CAP: Duration = Duration::from_millis(700);
pub const CPU_SAMPLE_WINDOW: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Default)]
pub struct PendingTransition {
    pub candidate: Option<AgentState>,
    pub confirmations: u8,
    pub started_at: Option<Instant>,
}

pub fn debounce(
    prev_public: AgentState,
    candidate: &EvidenceSummary,
    pending: &mut PendingTransition,
    now: Instant,
) -> Option<AgentState> {
    let next = public_state_for_raw(prev_public, candidate.raw_state);
    match next {
        AgentState::Blocked | AgentState::Working | AgentState::Unknown => {
            clear_pending(pending);
            publish_if_changed(prev_public, next)
        }
        AgentState::Idle | AgentState::Done
            if candidate.confidence >= AgentStatusConfidence::Strong =>
        {
            clear_pending(pending);
            publish_if_changed(prev_public, next)
        }
        AgentState::Idle | AgentState::Done => {
            if candidate.osc_progress_active || candidate.cpu_jiffies_delta > 0 {
                clear_pending(pending);
                return None;
            }
            if pending.candidate == Some(next) {
                pending.confirmations = pending.confirmations.saturating_add(1);
            } else {
                pending.candidate = Some(next);
                pending.confirmations = 1;
                pending.started_at = Some(now);
            }
            let capped = pending
                .started_at
                .is_some_and(|started| now.duration_since(started) >= IDLE_HOLD_CAP);
            if pending.confirmations >= IDLE_CONFIRMATIONS || capped {
                clear_pending(pending);
                publish_if_changed(prev_public, next)
            } else {
                None
            }
        }
    }
}

fn clear_pending(pending: &mut PendingTransition) {
    pending.candidate = None;
    pending.confirmations = 0;
    pending.started_at = None;
}

pub fn apply_watchdog(mut candidate: EvidenceSummary, now: Instant) -> EvidenceSummary {
    if candidate.raw_state != RawAgentState::Working {
        return candidate;
    }
    if candidate
        .notes
        .iter()
        .any(|note| matches!(note, EvidenceNote::WatchdogDemoted))
    {
        return candidate;
    }
    if !candidate.physics_sampled {
        return candidate;
    }
    let Some(last_output) = candidate.last_output else {
        return candidate;
    };
    if now.duration_since(last_output) < WATCHDOG_QUIET {
        return candidate;
    }
    if candidate.cpu_jiffies_delta > 0 || candidate.child_process_count > 0 {
        return candidate;
    }
    candidate.raw_state = RawAgentState::Unknown;
    candidate.confidence = AgentStatusConfidence::Unknown;
    candidate.winner = EvidenceWinner::Unknown;
    candidate.notes.push(EvidenceNote::WatchdogDemoted);
    candidate
}

fn publish_if_changed(prev: AgentState, next: AgentState) -> Option<AgentState> {
    (prev != next).then_some(next)
}

fn public_state_for_raw(prev_public: AgentState, raw: RawAgentState) -> AgentState {
    match raw {
        RawAgentState::Unknown => AgentState::Unknown,
        RawAgentState::Working => AgentState::Working,
        RawAgentState::Blocked => AgentState::Blocked,
        RawAgentState::Idle => {
            if matches!(prev_public, AgentState::Working | AgentState::Blocked) {
                AgentState::Done
            } else {
                AgentState::Idle
            }
        }
    }
}

#[cfg(test)]
mod tests;
