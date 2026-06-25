use super::*;
use crate::agent_status::evidence::{
    ActivityEvidence, AuthorityEvidence, AuthorityGrade, OscEvidence, ProcessEvidence,
    ScreenEvidence,
};
use std::time::Duration;

fn base_snapshot(now: Instant) -> EvidenceSnapshot {
    EvidenceSnapshot {
        authority: None,
        osc: OscEvidence::default(),
        screen: ScreenEvidence {
            observed_at: now,
            ..ScreenEvidence::default()
        },
        process: ProcessEvidence {
            child_alive: true,
            foreground_is_agent: true,
            ..ProcessEvidence::default()
        },
        activity: ActivityEvidence::default(),
        subagents_active: 0,
    }
}

fn authority(
    state: RawAgentState,
    pending_permission: bool,
    last_event: Instant,
) -> AuthorityEvidence {
    AuthorityEvidence {
        source_id: "hook-claude-1".to_owned(),
        grade: AuthorityGrade::Partial,
        mapped_state: state,
        pending_permission,
        last_event,
        notes: Vec::new(),
    }
}

#[test]
fn process_exit_wins() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.process.process_exited = true;
    snapshot.authority = Some(authority(RawAgentState::Working, false, now));

    let result = arbitrate(&snapshot, RawAgentState::Working, now);

    assert_eq!(result.raw, RawAgentState::Idle);
    assert_eq!(result.winner, EvidenceWinner::ProcessExit);
    assert!(result.summary.has_note(EvidenceNote::ProcessExited));
}

#[test]
fn foreground_shell_handoff_wins_as_exit_like_idle() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.process.foreground_returned_to_shell = true;
    snapshot.process.child_alive = true;
    snapshot.process.root_is_agent = false;
    snapshot.process.foreground_is_agent = false;
    snapshot.authority = Some(authority(RawAgentState::Working, false, now));

    let result = arbitrate(&snapshot, RawAgentState::Working, now);

    assert_eq!(result.raw, RawAgentState::Idle);
    assert_eq!(result.winner, EvidenceWinner::ProcessExit);
    assert!(result.summary.foreground_returned_to_shell);
    assert!(
        result
            .summary
            .has_note(EvidenceNote::ForegroundReturnedToShell)
    );
    assert!(!result.summary.stale_report);
}

#[test]
fn freeze_keeps_previous_raw() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.screen.freeze = true;
    snapshot.screen.state = Some(RawAgentState::Blocked);

    let result = arbitrate(&snapshot, RawAgentState::Working, now);

    assert_eq!(result.raw, RawAgentState::Working);
    assert_eq!(result.winner, EvidenceWinner::Freeze);
}

#[test]
fn pending_permission_blocks_immediately() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.authority = Some(authority(RawAgentState::Blocked, true, now));

    let result = arbitrate(&snapshot, RawAgentState::Working, now);

    assert_eq!(result.raw, RawAgentState::Blocked);
    assert_eq!(result.winner, EvidenceWinner::Blocked);
}

#[test]
fn fresh_screen_blocker_overrides_non_blocked_authority() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.authority = Some(authority(
        RawAgentState::Working,
        false,
        now.checked_sub(Duration::from_secs(1)).unwrap(),
    ));
    snapshot.screen.state = Some(RawAgentState::Blocked);
    snapshot.screen.strong = true;
    snapshot.screen.observed_at = now;

    let result = arbitrate(&snapshot, RawAgentState::Working, now);

    assert_eq!(result.raw, RawAgentState::Blocked);
    assert_eq!(result.winner, EvidenceWinner::Blocked);
}

#[test]
fn stale_screen_blocker_does_not_override_fresher_authority() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.authority = Some(authority(RawAgentState::Working, false, now));
    snapshot.screen.state = Some(RawAgentState::Blocked);
    snapshot.screen.strong = true;
    snapshot.screen.observed_at = now.checked_sub(Duration::from_secs(1)).unwrap();

    let result = arbitrate(&snapshot, RawAgentState::Idle, now);

    assert_eq!(result.raw, RawAgentState::Working);
    assert_eq!(result.winner, EvidenceWinner::Authority);
}

#[test]
fn fresh_authority_wins_after_blocker_checks() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.authority = Some(authority(RawAgentState::Working, false, now));

    let result = arbitrate(&snapshot, RawAgentState::Idle, now);

    assert_eq!(result.raw, RawAgentState::Working);
    assert_eq!(result.winner, EvidenceWinner::Authority);
}

#[test]
fn authority_confidence_reflects_grade() {
    let now = Instant::now();

    // Complete-grade runtime-event authority is the most trusted semantic
    // source -> Authoritative.
    let mut complete = base_snapshot(now);
    let mut a = authority(RawAgentState::Working, false, now);
    a.grade = AuthorityGrade::Complete;
    complete.authority = Some(a);
    assert_eq!(
        arbitrate(&complete, RawAgentState::Idle, now).confidence,
        AgentStatusConfidence::Authoritative,
    );

    // Partial-grade coverage cannot author at full confidence -> Strong.
    let mut partial = base_snapshot(now);
    let mut b = authority(RawAgentState::Working, false, now);
    b.grade = AuthorityGrade::Partial;
    partial.authority = Some(b);
    assert_eq!(
        arbitrate(&partial, RawAgentState::Idle, now).confidence,
        AgentStatusConfidence::Strong,
    );
}

#[test]
fn expired_authority_leaves_note_and_falls_back_unknown() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.authority = Some(authority(
        RawAgentState::Working,
        false,
        now.checked_sub(AUTHORITY_TTL + Duration::from_secs(1))
            .unwrap(),
    ));

    let result = arbitrate(&snapshot, RawAgentState::Working, now);

    assert_eq!(result.raw, RawAgentState::Unknown);
    assert!(result.summary.stale_report);
    assert!(result.summary.has_note(EvidenceNote::AuthorityExpired));
}

#[test]
fn identity_mismatch_leaves_note_and_rejects_authority() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.authority = Some(authority(RawAgentState::Working, false, now));
    snapshot.process.foreground_is_agent = false;

    let result = arbitrate(&snapshot, RawAgentState::Working, now);

    assert_eq!(result.raw, RawAgentState::Unknown);
    assert!(result.summary.stale_report);
    assert!(
        result
            .summary
            .has_note(EvidenceNote::AuthorityIdentityMismatch)
    );
}

#[test]
fn strong_screen_idle_wins_without_authority() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.screen.state = Some(RawAgentState::Idle);
    snapshot.screen.strong = true;

    let result = arbitrate(&snapshot, RawAgentState::Working, now);

    assert_eq!(result.raw, RawAgentState::Idle);
    assert_eq!(result.winner, EvidenceWinner::StrongVisualOrOsc);
}

#[test]
fn osc_progress_clear_is_idle_hint() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.osc.progress_cleared_at = Some(now);

    let result = arbitrate(&snapshot, RawAgentState::Working, now);

    assert_eq!(result.raw, RawAgentState::Idle);
    assert_eq!(result.winner, EvidenceWinner::StrongVisualOrOsc);
    assert_eq!(
        result.confidence,
        AgentStatusConfidence::Weak,
        "progress-clear is a done-ish hint: it must enter at Weak so the \
             debounce policy still requires idle confirmation, never Strong"
    );
    assert!(
        !result.summary.shell_integration,
        "agent-authored progress-clear must not be attributed to shell integration"
    );
}

#[test]
fn osc_shell_marker_is_shell_integration_evidence() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.osc.shell_state = Some(RawAgentState::Idle);

    let result = arbitrate(&snapshot, RawAgentState::Working, now);

    assert_eq!(result.raw, RawAgentState::Idle);
    assert_eq!(result.winner, EvidenceWinner::StrongVisualOrOsc);
    assert!(result.summary.shell_integration);
}

#[test]
fn osc_progress_clear_is_ignored_when_foreground_is_not_agent() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.process.foreground_is_agent = false;
    snapshot.osc.progress_cleared_at = Some(now);

    let result = arbitrate(&snapshot, RawAgentState::Working, now);

    assert_eq!(result.raw, RawAgentState::Unknown);
    assert_eq!(result.winner, EvidenceWinner::Unknown);
}

#[test]
fn physics_only_promotes_to_weak_working() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.process.child_process_count = 1;

    let result = arbitrate(&snapshot, RawAgentState::Unknown, now);

    assert_eq!(result.raw, RawAgentState::Working);
    assert_eq!(result.confidence, AgentStatusConfidence::Weak);
    assert_eq!(result.winner, EvidenceWinner::Physics);
}

#[test]
fn no_evidence_is_unknown() {
    let now = Instant::now();
    let mut snapshot = base_snapshot(now);
    snapshot.process.foreground_is_agent = false;

    let result = arbitrate(&snapshot, RawAgentState::Unknown, now);

    assert_eq!(result.raw, RawAgentState::Unknown);
    assert_eq!(result.winner, EvidenceWinner::Unknown);
}

#[test]
fn rollup_priority_matches_contract() {
    let states = [AgentState::Idle, AgentState::Working, AgentState::Done];
    assert_eq!(roll_up_states(states.iter()), AgentState::Done);
}
