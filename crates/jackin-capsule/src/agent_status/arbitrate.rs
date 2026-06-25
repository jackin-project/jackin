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
mod tests;
