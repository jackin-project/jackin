//! Pure arbitration from collected evidence to one raw agent state.

use std::borrow::Borrow;
use std::time::Instant;

use jackin_protocol::agent_status::AgentStatusConfidence;

use crate::agent_status::evidence::{
    AuthorityEvidence, EvidenceNote, EvidenceSnapshot, EvidenceSummary, EvidenceWinner,
    RawAgentState,
};
use crate::agent_status::policy::AUTHORITY_TTL;
use crate::protocol::AgentState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArbitrationResult {
    pub raw: RawAgentState,
    pub confidence: AgentStatusConfidence,
    pub winner: EvidenceWinner,
    pub notes: Vec<EvidenceNote>,
    pub summary: EvidenceSummary,
}

pub fn arbitrate(
    snapshot: &EvidenceSnapshot,
    previous_raw: RawAgentState,
    now: Instant,
) -> ArbitrationResult {
    let mut summary = EvidenceSummary {
        authority_source: snapshot
            .authority
            .as_ref()
            .map(|authority| authority.source_id.clone()),
        visible_blocker: snapshot.screen.state == Some(RawAgentState::Blocked),
        visible_idle: snapshot.screen.state == Some(RawAgentState::Idle),
        visible_working: snapshot.screen.state == Some(RawAgentState::Working),
        process_exited: snapshot.process.process_exited,
        foreground_returned_to_shell: snapshot.process.foreground_returned_to_shell,
        root_is_agent: snapshot.process.root_is_agent,
        foreground_pgid: snapshot.process.foreground_pgid,
        rule_id: snapshot.screen.rule_id.clone(),
        last_output: snapshot.activity.last_output,
        last_input: snapshot.activity.last_input,
        child_process_count: snapshot.process.child_process_count,
        cpu_jiffies_delta: snapshot.process.cpu_jiffies_delta,
        subagents_active: snapshot.subagents_active,
        osc_progress_active: snapshot.osc.progress_active,
        shell_integration: snapshot.osc.shell_state.is_some(),
        ..EvidenceSummary::default()
    };
    if let Some(authority) = &snapshot.authority {
        summary.notes.extend(authority.notes.clone());
    }

    if snapshot.process.process_exited || snapshot.process.foreground_returned_to_shell {
        if snapshot.process.process_exited {
            summary.notes.push(EvidenceNote::ProcessExited);
        }
        if snapshot.process.foreground_returned_to_shell {
            summary.notes.push(EvidenceNote::ForegroundReturnedToShell);
        }
        return finish(
            RawAgentState::Idle,
            AgentStatusConfidence::Weak,
            EvidenceWinner::ProcessExit,
            summary,
        );
    }

    if snapshot.screen.freeze {
        return finish(
            previous_raw,
            AgentStatusConfidence::Strong,
            EvidenceWinner::Freeze,
            summary,
        );
    }

    let fresh_authority = snapshot.authority.as_ref().filter(|authority| {
        now.duration_since(authority.last_event) <= AUTHORITY_TTL
            && snapshot.process.foreground_is_agent
    });
    if let Some(authority) = &snapshot.authority
        && fresh_authority.is_none()
    {
        summary.stale_report = true;
        if now.duration_since(authority.last_event) > AUTHORITY_TTL {
            summary.notes.push(EvidenceNote::AuthorityExpired);
        }
        if !snapshot.process.foreground_is_agent {
            summary.notes.push(EvidenceNote::AuthorityIdentityMismatch);
        }
    }

    if let Some(authority) = fresh_authority
        && authority.pending_permission
        && authority.mapped_state == RawAgentState::Blocked
    {
        return finish(
            RawAgentState::Blocked,
            authority_confidence(authority),
            EvidenceWinner::Blocked,
            summary,
        );
    }

    if snapshot.screen.strong && snapshot.screen.state == Some(RawAgentState::Blocked) {
        let screen_fresh_enough = fresh_authority.is_none_or(|authority| {
            snapshot.screen.observed_at >= authority.last_event
                || authority.mapped_state == RawAgentState::Blocked
        });
        if screen_fresh_enough {
            return finish(
                RawAgentState::Blocked,
                AgentStatusConfidence::Strong,
                EvidenceWinner::Blocked,
                summary,
            );
        }
    }

    if let Some(authority) = fresh_authority {
        return finish(
            authority.mapped_state,
            authority_confidence(authority),
            EvidenceWinner::Authority,
            summary,
        );
    }

    if let Some(shell_state) = snapshot.osc.shell_state {
        return finish(
            shell_state,
            AgentStatusConfidence::Strong,
            EvidenceWinner::StrongVisualOrOsc,
            summary,
        );
    }

    if snapshot.screen.strong
        && matches!(
            snapshot.screen.state,
            Some(RawAgentState::Working | RawAgentState::Idle)
        )
    {
        return finish(
            snapshot.screen.state.unwrap_or(RawAgentState::Unknown),
            AgentStatusConfidence::Strong,
            EvidenceWinner::StrongVisualOrOsc,
            summary,
        );
    }
    if snapshot.process.foreground_is_agent && snapshot.osc.progress_cleared_at.is_some() {
        return finish(
            RawAgentState::Idle,
            AgentStatusConfidence::Strong,
            EvidenceWinner::StrongVisualOrOsc,
            summary,
        );
    }

    if snapshot.process.child_process_count > 0 || snapshot.process.cpu_jiffies_delta > 0 {
        return finish(
            RawAgentState::Working,
            AgentStatusConfidence::Weak,
            EvidenceWinner::Physics,
            summary,
        );
    }

    finish(
        RawAgentState::Unknown,
        AgentStatusConfidence::Unknown,
        EvidenceWinner::Unknown,
        summary,
    )
}

fn authority_confidence(authority: &AuthorityEvidence) -> AgentStatusConfidence {
    if authority.direct_state_report {
        AgentStatusConfidence::Strong
    } else {
        AgentStatusConfidence::Authoritative
    }
}

fn finish(
    raw: RawAgentState,
    confidence: AgentStatusConfidence,
    winner: EvidenceWinner,
    mut summary: EvidenceSummary,
) -> ArbitrationResult {
    summary.raw_state = raw;
    summary.confidence = confidence;
    summary.winner = winner;
    ArbitrationResult {
        raw,
        confidence,
        winner,
        notes: summary.notes.clone(),
        summary,
    }
}

/// Attention priority used for tab/workspace roll-up.
pub fn attention_priority(state: AgentState) -> u8 {
    match state {
        AgentState::Blocked => 4,
        AgentState::Done => 3,
        AgentState::Working => 2,
        AgentState::Idle => 1,
        AgentState::Unknown => 0,
    }
}

/// Roll up a collection of session states to the most attention-worthy.
pub fn roll_up_states<I>(states: I) -> AgentState
where
    I: IntoIterator,
    I::Item: Borrow<AgentState>,
{
    states
        .into_iter()
        .max_by_key(|s| attention_priority(*s.borrow()))
        .map_or(AgentState::Unknown, |s| *s.borrow())
}

#[cfg(test)]
mod tests {
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
            direct_state_report: false,
            mapped_state: state,
            pending_permission,
            last_event,
            seq: 1,
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
    fn direct_state_report_is_lower_confidence_than_runtime_event_authority() {
        let now = Instant::now();
        let mut snapshot = base_snapshot(now);
        let mut direct_report = authority(RawAgentState::Working, false, now);
        direct_report.direct_state_report = true;
        snapshot.authority = Some(direct_report);

        let result = arbitrate(&snapshot, RawAgentState::Idle, now);

        assert_eq!(result.raw, RawAgentState::Working);
        assert_eq!(result.winner, EvidenceWinner::Authority);
        assert_eq!(result.confidence, AgentStatusConfidence::Strong);
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
}
