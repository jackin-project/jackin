// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn session_event_records_roundtrip_every_kind() {
    let kinds = [
        SessionEventKind::Subscribed,
        SessionEventKind::StateChanged {
            previous: AgentState::Idle,
        },
        SessionEventKind::Activity,
        SessionEventKind::Exited {
            reason: Some("exit status 1".to_owned()),
        },
        SessionEventKind::Exited { reason: None },
    ];
    for (seq, kind) in kinds.into_iter().enumerate() {
        let record = SessionEventRecord {
            seq: seq as u64,
            session: 1,
            agent: Some("claude".to_owned()),
            account_id: Some("acc-1".to_owned()),
            state: AgentState::Working,
            last_output_ms: Some(120),
            last_input_ms: None,
            kind,
        };
        let json = serde_json::to_string(&ServerMsg::SessionEvent {
            event: Box::new(record.clone()),
        })
        .unwrap();
        assert!(
            !json.contains("last_input_ms"),
            "an absent activity timestamp must be omitted: {json}"
        );
        match serde_json::from_str::<ServerMsg>(&json).unwrap() {
            ServerMsg::SessionEvent { event } => assert_eq!(*event, record),
            other => panic!("decoded wrong variant: {other:?}"),
        }
    }
}

#[test]
fn state_changed_carries_the_working_transition_the_host_waits_on() {
    // The host-side wait in the managed run is "an idle session became
    // Working": `previous` is the old state, `state` the new one. Guard the
    // direction so the two are never swapped on the wire.
    let json = serde_json::to_string(&SessionEventRecord {
        seq: 0,
        session: 1,
        agent: None,
        account_id: None,
        state: AgentState::Working,
        last_output_ms: Some(3),
        last_input_ms: Some(5),
        kind: SessionEventKind::StateChanged {
            previous: AgentState::Idle,
        },
    })
    .unwrap();
    let decoded: SessionEventRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.state, AgentState::Working);
    assert_eq!(decoded.kind.label(), "state_changed");
    assert!(matches!(
        decoded.kind,
        SessionEventKind::StateChanged {
            previous: AgentState::Idle
        }
    ));
}
