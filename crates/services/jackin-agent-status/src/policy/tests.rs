// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::evidence::{EvidenceSummary, EvidenceWinner};

fn candidate(raw: RawAgentState, confidence: AgentStatusConfidence) -> EvidenceSummary {
    EvidenceSummary {
        raw_state: raw,
        confidence,
        winner: EvidenceWinner::Unknown,
        ..Default::default()
    }
}

#[test]
fn watchdog_demotes_quiet_working() {
    let now = Instant::now();
    let mut c = candidate(RawAgentState::Working, AgentStatusConfidence::Authoritative);
    c.physics_sampled = true;
    c.last_output = now.checked_sub(WATCHDOG_QUIET + Duration::from_secs(1));

    let result = apply_watchdog(c, now);

    assert_eq!(result.raw_state, RawAgentState::Unknown);
    assert!(result.has_note(EvidenceNote::WatchdogDemoted));
}

#[test]
fn watchdog_does_not_fire_when_physics_unsampled() {
    let now = Instant::now();
    let mut c = candidate(RawAgentState::Working, AgentStatusConfidence::Authoritative);
    c.physics_sampled = false;
    c.last_output = now.checked_sub(WATCHDOG_QUIET + Duration::from_secs(1));

    let result = apply_watchdog(c, now);

    assert_eq!(result.raw_state, RawAgentState::Working);
}

#[test]
fn watchdog_does_not_fire_with_recent_output() {
    let now = Instant::now();
    let mut c = candidate(RawAgentState::Working, AgentStatusConfidence::Authoritative);
    c.physics_sampled = true;
    c.last_output = Some(now);

    let result = apply_watchdog(c, now);

    assert_eq!(result.raw_state, RawAgentState::Working);
}

#[test]
fn watchdog_does_not_fire_with_live_child_process() {
    let now = Instant::now();
    let mut c = candidate(RawAgentState::Working, AgentStatusConfidence::Authoritative);
    c.physics_sampled = true;
    c.last_output = now.checked_sub(WATCHDOG_QUIET + Duration::from_secs(1));
    c.child_process_count = 1;

    let result = apply_watchdog(c, now);

    assert_eq!(result.raw_state, RawAgentState::Working);
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
fn inferred_idle_releases_at_hold_cap_without_three_confirmations() {
    let mut pending = PendingTransition::default();
    let c = candidate(RawAgentState::Idle, AgentStatusConfidence::Weak);
    let start = Instant::now();
    assert_eq!(debounce(AgentState::Working, &c, &mut pending, start), None);
    let capped = start + IDLE_HOLD_CAP + Duration::from_millis(1);
    assert_eq!(
        debounce(AgentState::Working, &c, &mut pending, capped),
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

    c.osc_progress_active = true;
    assert_eq!(debounce(AgentState::Working, &c, &mut pending, now), None);
    assert!(pending.candidate.is_none());

    c.osc_progress_active = false;
    c.cpu_jiffies_delta = 5;
    assert_eq!(debounce(AgentState::Working, &c, &mut pending, now), None);
    assert!(pending.candidate.is_none());
}
