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
/// Wall-clock cap on the inferred working→idle hold. A stuck confirmation loop
/// (state oscillating so the count never reaches `IDLE_CONFIRMATIONS`) must not
/// pin a stale `working` indefinitely; after this long the idle candidate is
/// released regardless of count. (Herdr-validated bound.)
pub const IDLE_HOLD_CAP: Duration = Duration::from_millis(700);
pub const CPU_SAMPLE_WINDOW: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Default)]
pub struct PendingTransition {
    pub candidate: Option<AgentState>,
    pub confirmations: u8,
    /// When the current inferred-idle hold began. Used to enforce `IDLE_HOLD_CAP`.
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
        // Blocked, working, exit, and positively-matched (Strong) idle publish
        // immediately. Strong idle clears any pending inferred-idle hold.
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
        // Inferred idle (absence of working chrome): require IDLE_CONFIRMATIONS
        // consecutive evaluations AND no OSC progress active AND CPU-quiet, with
        // a wall-clock cap so a never-confirming loop cannot pin stale working.
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
    // Linux-gate: the watchdog demotes "working with no physical activity", so it
    // may only fire when `/proc` physics was actually sampled. When physics is
    // unavailable (non-Linux, or the agent PID is unknown), a zero CPU/child
    // count means "no evidence", not "quiet" — demoting then would turn every
    // real working state into Unknown on the developer's own machine.
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
    // The early return above guarantees `notes` did not already carry
    // WatchdogDemoted, and nothing since added it, so this push is unconditional.
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
