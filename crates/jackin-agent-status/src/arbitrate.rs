//! Pure arbitration from collected evidence to one raw agent state.

use std::borrow::Borrow;
use std::time::Instant;

use jackin_protocol::agent_status::AgentStatusConfidence;

use crate::evidence::{
    AuthorityEvidence, AuthorityGrade, EvidenceNote, EvidenceSnapshot, EvidenceSummary,
    EvidenceWinner, RawAgentState,
};
use crate::policy::AUTHORITY_TTL;
use jackin_protocol::control::AgentState;

pub fn arbitrate(
    snapshot: &EvidenceSnapshot,
    previous_raw: RawAgentState,
    now: Instant,
) -> EvidenceSummary {
    let mut summary = EvidenceSummary {
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
        physics_sampled: snapshot.process.physics_sampled,
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
        // Process exit / foreground-return-to-shell is a definitive idle, not an
        // inferred one: publish immediately (Strong) so the done transition lands
        // on this tick, before the daemon clears runtime authority for an exiting
        // session. Weak here would route through the debounce idle-confirmation
        // path, which the same-tick authority clear then starves — the pane would
        // fall to Unknown instead of Done.
        return finish(
            RawAgentState::Idle,
            AgentStatusConfidence::Strong,
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
            EvidenceWinner::Authority {
                source_id: authority.source_id.clone(),
            },
            summary,
        );
    }

    if snapshot.screen.strong && snapshot.screen.state == Some(RawAgentState::Blocked) {
        // A strong Blocked match is a dialog visible on the live screen right now
        // — the rule packs match the bottom region, not scrollback — so it is
        // current ground truth and overrides a runtime authority that may be
        // reporting a now-superseded Working/Idle.
        return finish(
            RawAgentState::Blocked,
            AgentStatusConfidence::Strong,
            EvidenceWinner::Blocked,
            summary,
        );
    }

    if let Some(authority) = fresh_authority {
        return finish(
            authority.mapped_state,
            authority_confidence(authority),
            EvidenceWinner::Authority {
                source_id: authority.source_id.clone(),
            },
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
        && let Some(state @ (RawAgentState::Working | RawAgentState::Idle)) = snapshot.screen.state
    {
        return finish(
            state,
            AgentStatusConfidence::Strong,
            EvidenceWinner::StrongVisualOrOsc,
            summary,
        );
    }
    // OSC 9;4 progress-clear is a done-ish *hint*, never authoritative idle: a
    // single clear edge enters at Weak confidence so the debounce policy still
    // requires IDLE_CONFIRMATIONS consecutive evaluations (and CPU-quiet) before
    // publishing idle. Promoting it to Strong here would bypass that and flip a
    // still-working agent to idle on one progress edge.
    if snapshot.process.foreground_is_agent && snapshot.osc.progress_cleared_at.is_some() {
        return finish(
            RawAgentState::Idle,
            AgentStatusConfidence::Weak,
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
    // Daemon-mapped runtime-event authority, graded by lifecycle coverage:
    // Complete (e.g. OpenCode's full event stream) is the most trusted semantic
    // source; Partial coverage cannot author at full confidence.
    match authority.grade {
        AuthorityGrade::Complete => AgentStatusConfidence::Authoritative,
        AuthorityGrade::Partial => AgentStatusConfidence::Strong,
    }
}

fn finish(
    raw: RawAgentState,
    confidence: AgentStatusConfidence,
    winner: EvidenceWinner,
    mut summary: EvidenceSummary,
) -> EvidenceSummary {
    summary.raw_state = raw;
    summary.confidence = confidence;
    summary.winner = winner;
    summary
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
