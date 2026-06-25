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
    c.summary.physics_sampled = true;
    c.summary.last_output = now.checked_sub(WATCHDOG_QUIET + Duration::from_secs(1));

    let result = apply_watchdog(c, now);

    assert_eq!(result.raw, RawAgentState::Unknown);
    assert!(result.notes.contains(&EvidenceNote::WatchdogDemoted));
    assert!(result.summary.has_note(EvidenceNote::WatchdogDemoted));
}

#[test]
fn watchdog_does_not_fire_when_physics_unsampled() {
    // Linux-gate: with physics unavailable (non-Linux, or agent PID unknown)
    // zero CPU/child counts mean "no evidence", not "quiet". The watchdog
    // must hold the working state, not demote a real working agent.
    let now = Instant::now();
    let mut c = candidate(RawAgentState::Working, AgentStatusConfidence::Authoritative);
    c.summary.physics_sampled = false;
    c.summary.last_output = now.checked_sub(WATCHDOG_QUIET + Duration::from_secs(1));

    let result = apply_watchdog(c, now);

    assert_eq!(result.raw, RawAgentState::Working);
}

#[test]
fn watchdog_does_not_fire_with_recent_output() {
    let now = Instant::now();
    let mut c = candidate(RawAgentState::Working, AgentStatusConfidence::Authoritative);
    c.summary.physics_sampled = true;
    c.summary.last_output = Some(now);

    let result = apply_watchdog(c, now);

    assert_eq!(result.raw, RawAgentState::Working);
}

#[test]
fn watchdog_does_not_fire_with_live_child_process() {
    let now = Instant::now();
    let mut c = candidate(RawAgentState::Working, AgentStatusConfidence::Authoritative);
    c.summary.physics_sampled = true;
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
fn inferred_idle_is_blocked_by_osc_progress_or_cpu() {
    let now = Instant::now();
    let mut pending = PendingTransition::default();
    let mut c = candidate(RawAgentState::Idle, AgentStatusConfidence::Weak);

    // OSC 9;4 progress still animating -> never an inferred idle, hold reset.
    c.summary.osc_progress_active = true;
    assert_eq!(debounce(AgentState::Working, &c, &mut pending, now), None);
    assert!(pending.candidate.is_none());

    // CPU still burning -> same.
    c.summary.osc_progress_active = false;
    c.summary.cpu_jiffies_delta = 5;
    assert_eq!(debounce(AgentState::Working, &c, &mut pending, now), None);
    assert!(pending.candidate.is_none());
}
