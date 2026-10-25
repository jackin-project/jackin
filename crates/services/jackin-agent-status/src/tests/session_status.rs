// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::SessionStatus;
use crate::evidence::{EvidenceSummary, RawAgentState};
use jackin_protocol::control::AgentState;

fn summary_for(raw: RawAgentState) -> EvidenceSummary {
    EvidenceSummary {
        raw_state: raw,
        ..EvidenceSummary::default()
    }
}

#[test]
fn first_working_publish_reports_transition() {
    let mut status = SessionStatus::new();
    let event = status.publish_raw(summary_for(RawAgentState::Working));
    assert_eq!(event, Some(AgentState::Working));
    assert_eq!(status.effective, AgentState::Working);
    assert_eq!(status.revision, 1);
}

#[test]
fn repeat_publish_reports_no_change() {
    let mut status = SessionStatus::new();
    assert_eq!(
        status.publish_raw(summary_for(RawAgentState::Working)),
        Some(AgentState::Working)
    );
    assert_eq!(
        status.publish_raw(summary_for(RawAgentState::Working)),
        None
    );
    assert_eq!(status.revision, 1);
}

#[test]
fn unseen_idle_becomes_done_until_acknowledged() {
    let mut status = SessionStatus::new();
    assert_eq!(
        status.publish_raw(summary_for(RawAgentState::Working)),
        Some(AgentState::Working)
    );
    assert_eq!(
        status.publish_raw(summary_for(RawAgentState::Idle)),
        Some(AgentState::Done)
    );
    assert_eq!(status.effective, AgentState::Done);
    assert_eq!(status.acknowledge(), Some(AgentState::Idle));
    assert_eq!(status.effective, AgentState::Idle);
}
