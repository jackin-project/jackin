// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn report_attributes_source_by_winner_when_authority_did_not_win() {
    // For every non-authority winner, report() maps the source from the winning
    // channel — never Reported (that is reserved for EvidenceWinner::Authority).
    let cases = [
        (
            evidence::EvidenceWinner::Physics,
            false,
            AgentStatusSource::ForegroundProcess,
        ),
        (
            evidence::EvidenceWinner::StrongVisualOrOsc,
            false,
            AgentStatusSource::VisibleScreen,
        ),
        (
            evidence::EvidenceWinner::StrongVisualOrOsc,
            true,
            AgentStatusSource::ShellIntegration,
        ),
        (
            evidence::EvidenceWinner::Blocked,
            false,
            AgentStatusSource::VisibleScreen,
        ),
        (
            evidence::EvidenceWinner::Freeze,
            false,
            AgentStatusSource::VisibleScreen,
        ),
        (
            evidence::EvidenceWinner::ProcessExit,
            false,
            AgentStatusSource::None,
        ),
        (
            evidence::EvidenceWinner::Unknown,
            false,
            AgentStatusSource::None,
        ),
    ];
    for (winner, shell_integration, expected) in cases {
        let mut s = SessionStatus::new();
        s.publish_raw(EvidenceSummary {
            raw_state: RawAgentState::Working,
            confidence: AgentStatusConfidence::Strong,
            winner: winner.clone(),
            shell_integration,
            ..EvidenceSummary::default()
        });
        assert_eq!(
            s.report(None).source,
            expected,
            "winner {winner:?} should map to {expected:?}"
        );
    }
}

#[test]
fn roll_up_priority_blocked_gt_done_gt_working_gt_idle_gt_unknown() {
    use crate::arbitrate::attention_priority;
    assert!(attention_priority(AgentState::Blocked) > attention_priority(AgentState::Done));
    assert!(attention_priority(AgentState::Done) > attention_priority(AgentState::Working));
    assert!(attention_priority(AgentState::Working) > attention_priority(AgentState::Idle));
    assert!(attention_priority(AgentState::Idle) > attention_priority(AgentState::Unknown));
}

#[test]
fn multiple_sessions_roll_up_reflects_most_urgent() {
    use crate::arbitrate::roll_up_states;

    let session_states = vec![
        AgentState::Working,
        AgentState::Blocked,
        AgentState::Working,
        AgentState::Idle,
    ];
    let rolled = roll_up_states(&session_states);
    assert_eq!(rolled, AgentState::Blocked);
}
