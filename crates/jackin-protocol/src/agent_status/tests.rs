// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `agent_status`.
use super::*;

#[test]
fn confidence_orders_weakest_to_strongest() {
    // The debounce policy compares confidences with `>=` / `<`, so this order
    // is load-bearing.
    assert!(AgentStatusConfidence::Unknown < AgentStatusConfidence::Weak);
    assert!(AgentStatusConfidence::Weak < AgentStatusConfidence::Strong);
    assert!(AgentStatusConfidence::Strong < AgentStatusConfidence::Authoritative);
}

#[test]
fn report_roundtrips_with_reported_source() {
    let report = AgentStatusReport {
        raw_state: AgentRawState::Working,
        source: AgentStatusSource::Reported {
            source_id: "hook-opencode-1".to_owned(),
        },
        confidence: AgentStatusConfidence::Authoritative,
        detected_agent: Some("opencode".to_owned()),
        foreground_pgid: Some(42),
        visible_blocker: false,
        visible_idle: false,
        visible_working: true,
        process_exited: false,
        foreground_returned_to_shell: false,
        stale_report: false,
        subagents_active: 2,
        revision: 5,
    };
    let json = serde_json::to_string(&report).unwrap();
    let decoded: AgentStatusReport = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.raw_state, AgentRawState::Working);
    assert_eq!(decoded.confidence, AgentStatusConfidence::Authoritative);
    assert_eq!(decoded.subagents_active, 2);
    assert!(matches!(
        decoded.source,
        AgentStatusSource::Reported { source_id } if source_id == "hook-opencode-1"
    ));
}
