use std::time::{Duration, Instant};

use jackin_protocol::agent_status::AgentStatusConfidence;

use crate::agent_status::arbitrate::ArbitrationResult;
use crate::agent_status::evidence::{EvidenceNote, EvidenceWinner, RawAgentState};
use crate::protocol::AgentState;

pub const AUTHORITY_TTL: Duration = Duration::from_secs(30);
pub const WATCHDOG_QUIET: Duration = Duration::from_secs(10);
pub const IDLE_CONFIRMATIONS: u8 = 3;
pub const STARTUP_GRACE: Duration = Duration::from_secs(3);
pub const CPU_SAMPLE_WINDOW: Duration = Duration::from_secs(2);
pub const RENOTIFY_INTERVAL: Duration = Duration::from_mins(5);
pub const EVAL_COALESCE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Default)]
pub struct PendingTransition {
    pub candidate: Option<AgentState>,
    pub confirmations: u8,
}

pub fn debounce(
    prev_public: AgentState,
    candidate: &ArbitrationResult,
    pending: &mut PendingTransition,
    _now: Instant,
) -> Option<AgentState> {
    let next = public_state_for_raw(prev_public, candidate.raw);
    match next {
        AgentState::Blocked | AgentState::Working => publish_if_changed(prev_public, next),
        AgentState::Idle | AgentState::Done
            if candidate.confidence >= AgentStatusConfidence::Strong =>
        {
            pending.candidate = None;
            pending.confirmations = 0;
            publish_if_changed(prev_public, next)
        }
        AgentState::Idle | AgentState::Done => {
            if pending.candidate == Some(next) {
                pending.confirmations = pending.confirmations.saturating_add(1);
            } else {
                pending.candidate = Some(next);
                pending.confirmations = 1;
            }
            if pending.confirmations >= IDLE_CONFIRMATIONS {
                publish_if_changed(prev_public, next)
            } else {
                None
            }
        }
        AgentState::Unknown => publish_if_changed(prev_public, next),
    }
}

pub fn should_publish_candidate(
    prev_public: AgentState,
    candidate: &ArbitrationResult,
    pending: &mut PendingTransition,
) -> bool {
    if candidate.winner == EvidenceWinner::ProcessExit {
        pending.candidate = None;
        pending.confirmations = 0;
        return true;
    }
    let next = public_state_for_raw(prev_public, candidate.raw);
    match next {
        AgentState::Idle | AgentState::Done
            if candidate.confidence < AgentStatusConfidence::Strong =>
        {
            if candidate.summary.osc_progress_active || candidate.summary.cpu_jiffies_delta > 0 {
                pending.candidate = None;
                pending.confirmations = 0;
                return false;
            }
            if pending.candidate == Some(next) {
                pending.confirmations = pending.confirmations.saturating_add(1);
            } else {
                pending.candidate = Some(next);
                pending.confirmations = 1;
            }
            pending.confirmations >= IDLE_CONFIRMATIONS
        }
        _ => {
            pending.candidate = None;
            pending.confirmations = 0;
            true
        }
    }
}

pub fn apply_watchdog(mut candidate: ArbitrationResult, now: Instant) -> ArbitrationResult {
    if candidate.raw != RawAgentState::Working {
        return candidate;
    }
    if candidate
        .summary
        .notes
        .iter()
        .any(|note| matches!(note, EvidenceNote::WatchdogDemoted))
    {
        return candidate;
    }
    let Some(last_output) = candidate.summary.last_output else {
        return candidate;
    };
    if now.duration_since(last_output) < WATCHDOG_QUIET {
        return candidate;
    }
    if candidate.summary.cpu_jiffies_delta > 0 || candidate.summary.child_process_count > 0 {
        return candidate;
    }
    candidate.raw = RawAgentState::Unknown;
    candidate.confidence = AgentStatusConfidence::Unknown;
    candidate.winner = EvidenceWinner::Unknown;
    candidate.notes.push(EvidenceNote::WatchdogDemoted);
    candidate.summary.raw_state = RawAgentState::Unknown;
    candidate.summary.confidence = AgentStatusConfidence::Unknown;
    candidate.summary.winner = EvidenceWinner::Unknown;
    if !candidate
        .summary
        .notes
        .iter()
        .any(|note| matches!(note, EvidenceNote::WatchdogDemoted))
    {
        candidate.summary.notes.push(EvidenceNote::WatchdogDemoted);
    }
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
mod tests {
    use super::*;
    use crate::agent_status::evidence::{EvidenceSummary, EvidenceWinner};

    fn candidate(raw: RawAgentState, confidence: AgentStatusConfidence) -> ArbitrationResult {
        ArbitrationResult {
            raw,
            confidence,
            winner: EvidenceWinner::Unknown,
            notes: Vec::new(),
            summary: EvidenceSummary {
                raw_state: raw,
                confidence,
                ..Default::default()
            },
        }
    }

    #[test]
    fn watchdog_demotes_quiet_working() {
        let now = Instant::now();
        let mut c = candidate(RawAgentState::Working, AgentStatusConfidence::Authoritative);
        c.summary.last_output = now.checked_sub(WATCHDOG_QUIET + Duration::from_secs(1));

        let result = apply_watchdog(c, now);

        assert_eq!(result.raw, RawAgentState::Unknown);
        assert!(result.notes.contains(&EvidenceNote::WatchdogDemoted));
        assert!(result.summary.has_note(EvidenceNote::WatchdogDemoted));
    }

    #[test]
    fn watchdog_does_not_fire_with_recent_output() {
        let now = Instant::now();
        let mut c = candidate(RawAgentState::Working, AgentStatusConfidence::Authoritative);
        c.summary.last_output = Some(now);

        let result = apply_watchdog(c, now);

        assert_eq!(result.raw, RawAgentState::Working);
    }

    #[test]
    fn watchdog_does_not_fire_with_live_child_process() {
        let now = Instant::now();
        let mut c = candidate(RawAgentState::Working, AgentStatusConfidence::Authoritative);
        c.summary.last_output = now.checked_sub(WATCHDOG_QUIET + Duration::from_secs(1));
        c.summary.child_process_count = 1;

        let result = apply_watchdog(c, now);

        assert_eq!(result.raw, RawAgentState::Working);
    }

    #[test]
    fn blocked_publishes_immediately() {
        let mut pending = PendingTransition::default();
        let result = debounce(
            AgentState::Working,
            &candidate(RawAgentState::Blocked, AgentStatusConfidence::Strong),
            &mut pending,
            Instant::now(),
        );
        assert_eq!(result, Some(AgentState::Blocked));
    }

    #[test]
    fn inferred_idle_needs_three_confirmations() {
        let mut pending = PendingTransition::default();
        let c = candidate(RawAgentState::Idle, AgentStatusConfidence::Weak);
        assert_eq!(
            debounce(AgentState::Working, &c, &mut pending, Instant::now()),
            None
        );
        assert_eq!(
            debounce(AgentState::Working, &c, &mut pending, Instant::now()),
            None
        );
        assert_eq!(
            debounce(AgentState::Working, &c, &mut pending, Instant::now()),
            Some(AgentState::Done)
        );
    }

    #[test]
    fn visible_idle_publishes_immediately() {
        let mut pending = PendingTransition::default();
        let result = debounce(
            AgentState::Working,
            &candidate(RawAgentState::Idle, AgentStatusConfidence::Strong),
            &mut pending,
            Instant::now(),
        );
        assert_eq!(result, Some(AgentState::Done));
    }

    #[test]
    fn inferred_idle_publication_is_held_until_confirmed() {
        let mut pending = PendingTransition::default();
        let c = candidate(RawAgentState::Idle, AgentStatusConfidence::Weak);
        assert!(!should_publish_candidate(
            AgentState::Working,
            &c,
            &mut pending
        ));
        assert!(!should_publish_candidate(
            AgentState::Working,
            &c,
            &mut pending
        ));
        assert!(should_publish_candidate(
            AgentState::Working,
            &c,
            &mut pending
        ));
    }
}
