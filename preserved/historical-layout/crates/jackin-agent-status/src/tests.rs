// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn osc_status_decoder_matches_wire_events_at_every_split() {
    use OscShellMark::{CommandFinished, PreExec, PromptEnd, PromptStart};
    use OscStatusEvent::{Progress, Shell};
    let wire = b"text\x1b]133;A\x07\x1b]133;B\x1b\\\x1b]133;C\x07\x1b]133;D;127\x1b\\\x1b]9;4;1;50\x07\x1b]9;4;0\x1b\\";
    let expected = vec![
        Shell(PromptStart),
        Shell(PromptEnd),
        Shell(PreExec),
        Shell(CommandFinished {
            exit_code: Some(127),
        }),
        Progress(1),
        Progress(0),
    ];
    for split in 0..=wire.len() {
        let mut decoder = OscStatusDecoder::default();
        let mut actual = decoder.feed(&wire[..split]);
        actual.extend(decoder.feed(&wire[split..]));
        assert_eq!(actual, expected, "split {split}");
    }
    let mut decoder = OscStatusDecoder::default();
    let actual: Vec<_> = wire
        .chunks(1)
        .flat_map(|chunk| decoder.feed(chunk))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn osc_status_decoder_waits_for_a_complete_terminator() {
    let mut decoder = OscStatusDecoder::default();
    assert!(decoder.feed(b"\x1b]133;D;12").is_empty());
    assert!(decoder.feed(b"7\x1b").is_empty());
    assert_eq!(
        decoder.feed(b"\\"),
        vec![OscStatusEvent::Shell(OscShellMark::CommandFinished {
            exit_code: Some(127)
        })]
    );
}

#[test]
fn osc_status_decoder_discards_malformed_and_recovers() {
    for malformed in [
        b"\x1b ]133;C\x07".as_slice(),
        b"\x1b]133;C\x18\x07",
        b"\x1b]133;C\x1a\x07",
        b"\x1b]133;D;12oops\x07",
        b"\x1b]133;D;2147483648\x07",
        b"\x1b]133;Cat\x07",
        b"\x1b]9;4;12\x07",
        b"\x9d133;C\x07",
        b"\xc2\x9d133;C\x07",
        b"\x1b]133;C\x9c\x07",
    ] {
        let mut decoder = OscStatusDecoder::default();
        assert!(decoder.feed(malformed).is_empty(), "{malformed:?}");
        assert_eq!(
            decoder.feed(b"\x1b]133;B\x07"),
            vec![OscStatusEvent::Shell(OscShellMark::PromptEnd)]
        );
    }
    let mut decoder = OscStatusDecoder::default();
    assert!(decoder.feed(b"\x1b]133;C;").is_empty());
    assert!(decoder.feed(&[b'x'; 4096]).is_empty());
    assert!(decoder.feed(b"\x07").is_empty());
    assert_eq!(
        decoder.feed(b"\x1b]133;C\x07"),
        vec![OscStatusEvent::Shell(OscShellMark::PreExec)]
    );
}

#[test]
fn osc_status_decoder_recovers_from_abandoned_strings() {
    let expected = vec![OscStatusEvent::Shell(OscShellMark::PreExec)];
    for prefix in [
        b"\x1b]unfinished".as_slice(),
        b"\x1bPdata",
        b"\x1bXdata",
        b"\x1b^data",
        b"\x1b_data",
    ] {
        let mut decoder = OscStatusDecoder::default();
        assert!(decoder.feed(prefix).is_empty());
        assert_eq!(decoder.feed(b"\x1b]133;C;aid=1\x07"), expected);
    }
}

#[test]
fn osc_status_decoder_handles_optional_codes_and_payload_boundary() {
    use OscShellMark::{CommandFinished, PreExec};
    use OscStatusEvent::Shell;
    let mut decoder = OscStatusDecoder::default();
    assert_eq!(
        decoder.feed(b"\x1b]133;D\x07\x1b]133;D;-1;aid=1\x07"),
        vec![
            Shell(CommandFinished { exit_code: None }),
            Shell(CommandFinished {
                exit_code: Some(-1)
            }),
        ]
    );
    for payload_len in [64, 65] {
        let mut wire = b"\x1b]133;C;".to_vec();
        wire.extend(vec![b'x'; payload_len - 6]);
        wire.push(7);
        let expected = if payload_len == 64 {
            vec![Shell(PreExec)]
        } else {
            vec![]
        };
        assert_eq!(decoder.feed(&wire), expected);
    }
    assert!(decoder.feed(b"\x1b]133;C;").is_empty());
    assert!(decoder.feed(&[b'x'; 65]).is_empty());
    assert_eq!(decoder.feed(b"\x1b]133;C\x07"), vec![Shell(PreExec)]);
    assert_eq!(decoder.feed(b"\x1b]133;\x00C\x07"), vec![Shell(PreExec)]);
}

#[test]
fn new_session_starts_unknown() {
    let s = SessionStatus::new();
    assert_eq!(s.effective, AgentState::Unknown);
    assert_eq!(s.raw, RawAgentState::Unknown);
    assert_eq!(s.revision, 0);
}

#[test]
fn publish_working_transitions_unknown_to_working() {
    let mut s = SessionStatus::new();
    let changed = s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Working,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    assert_eq!(changed, Some(AgentState::Working));
    assert_eq!(s.effective, AgentState::Working);
    assert_eq!(s.raw, RawAgentState::Working);
    assert!(!s.seen);
    assert_eq!(s.revision, 1);
}

#[test]
fn idle_after_working_produces_done_when_unseen() {
    let mut s = SessionStatus::new();
    s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Working,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    let changed = s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Idle,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    assert_eq!(changed, Some(AgentState::Done));
    assert_eq!(s.effective, AgentState::Done);
}

#[test]
fn repeated_idle_keeps_done_until_acknowledged() {
    let mut s = SessionStatus::new();
    s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Working,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Idle,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    assert_eq!(s.effective, AgentState::Done);

    let changed = s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Idle,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });

    assert_eq!(changed, None);
    assert_eq!(s.effective, AgentState::Done);
    assert!(!s.seen);
}

#[test]
fn idle_after_working_produces_idle_when_seen() {
    let mut s = SessionStatus::new();
    s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Working,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    s.seen = true;
    let changed = s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Idle,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    assert_eq!(changed, Some(AgentState::Idle));
}

#[test]
fn acknowledge_transitions_done_to_idle() {
    let mut s = SessionStatus::new();
    s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Working,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Idle,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    assert_eq!(s.effective, AgentState::Done);
    let changed = s.acknowledge();
    assert_eq!(changed, Some(AgentState::Idle));
    assert_eq!(s.effective, AgentState::Idle);
    assert!(s.seen);
}

#[test]
fn revision_increments_only_on_public_state_change() {
    let mut s = SessionStatus::new();
    assert_eq!(s.revision, 0);
    s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Working,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    assert_eq!(s.revision, 1);
    s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Working,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    assert_eq!(s.revision, 1);
    s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Idle,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    assert_eq!(s.revision, 2);
}

#[test]
fn blocked_enters_work_cycle_and_done_on_idle() {
    let mut s = SessionStatus::new();
    s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Blocked,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    assert_eq!(s.effective, AgentState::Blocked);
    assert!(!s.seen);
    let changed = s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Idle,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    assert_eq!(changed, Some(AgentState::Done));
}

#[test]
fn re_work_after_ack_creates_new_done() {
    let mut s = SessionStatus::new();
    s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Working,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Idle,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    assert_eq!(s.effective, AgentState::Done);
    s.acknowledge();
    assert_eq!(s.effective, AgentState::Idle);
    s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Working,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    let changed = s.publish_raw(EvidenceSummary {
        raw_state: RawAgentState::Idle,
        confidence: AgentStatusConfidence::Strong,
        ..Default::default()
    });
    assert_eq!(changed, Some(AgentState::Done));
}

#[test]
fn publish_raw_keeps_latest_evidence_summary() {
    let mut s = SessionStatus::new();
    let summary = EvidenceSummary {
        raw_state: RawAgentState::Blocked,
        confidence: AgentStatusConfidence::Strong,
        rule_id: Some("claude.permission-dialog".to_owned()),
        visible_blocker: true,
        ..EvidenceSummary::default()
    };
    s.publish_raw(summary);
    assert_eq!(s.last_snapshot_summary.raw_state, RawAgentState::Blocked);
    assert_eq!(
        s.last_snapshot_summary.confidence,
        AgentStatusConfidence::Strong
    );
    assert_eq!(
        s.last_snapshot_summary.rule_id.as_deref(),
        Some("claude.permission-dialog")
    );
    assert!(s.last_snapshot_summary.visible_blocker);
}

#[test]
fn report_uses_evidence_summary() {
    let mut s = SessionStatus::new();
    let summary = EvidenceSummary {
        raw_state: RawAgentState::Working,
        confidence: AgentStatusConfidence::Authoritative,
        winner: evidence::EvidenceWinner::Authority {
            source_id: "hook-claude-1".to_owned(),
        },
        foreground_pgid: Some(42),
        visible_working: true,
        subagents_active: 2,
        ..EvidenceSummary::default()
    };
    s.publish_raw(summary);
    let report = s.report(Some("claude".to_owned()));
    assert_eq!(report.raw_state, RawAgentState::Working);
    assert_eq!(report.confidence, AgentStatusConfidence::Authoritative);
    assert_eq!(report.foreground_pgid, Some(42));
    assert!(report.visible_working);
    assert_eq!(report.subagents_active, 2);
    assert_eq!(
        report.source,
        AgentStatusSource::Reported {
            source_id: "hook-claude-1".to_owned()
        }
    );
}

#[test]
fn report_preserves_shell_integration_source() {
    let mut s = SessionStatus::new();
    let summary = EvidenceSummary {
        raw_state: RawAgentState::Idle,
        confidence: AgentStatusConfidence::Strong,
        winner: evidence::EvidenceWinner::StrongVisualOrOsc,
        shell_integration: true,
        ..EvidenceSummary::default()
    };
    s.publish_raw(summary);

    assert_eq!(
        s.report(Some("codex".to_owned())).source,
        AgentStatusSource::ShellIntegration
    );
}

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
