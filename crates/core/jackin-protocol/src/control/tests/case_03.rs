// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn message_envelopes_keep_unknown_fallbacks_and_require_type() {
    assert!(matches!(
        serde_json::from_str::<ClientMsg>(r#"{"type":"future_query"}"#).unwrap(),
        ClientMsg::Unknown
    ));
    assert!(matches!(
        serde_json::from_str::<ServerMsg>(r#"{"type":"future_reply"}"#).unwrap(),
        ServerMsg::Unknown
    ));
    serde_json::from_str::<ClientMsg>(r#"{"foo":"bar"}"#).unwrap_err();
    serde_json::from_str::<ServerMsg>(r#"{"type":42}"#).unwrap_err();

    let status_wire = serde_json::to_value(ClientMsg::Status).unwrap();
    assert_eq!(status_wire, serde_json::json!({"type": "status"}));
    assert!(matches!(
        serde_json::from_value::<ClientMsg>(status_wire).unwrap(),
        ClientMsg::Status
    ));
}

#[test]
fn ordinary_messages_preserve_optional_payload_and_dispatch_metadata() {
    let event_without_payload = ClientMsg::ReportRuntimeEvent {
        session_id: 7,
        source_id: "hook-claude-7".to_owned(),
        runtime: "claude".to_owned(),
        event: "Stop".to_owned(),
        payload: None,
    };
    let event_without_payload_wire = serde_json::to_value(&event_without_payload).unwrap();
    assert_eq!(
        event_without_payload_wire,
        serde_json::json!({
            "type": "report_runtime_event",
            "session_id": 7,
            "source_id": "hook-claude-7",
            "runtime": "claude",
            "event": "Stop"
        })
    );
    assert!(event_without_payload_wire.get("payload").is_none());
    match serde_json::from_value::<ClientMsg>(event_without_payload_wire).unwrap() {
        ClientMsg::ReportRuntimeEvent {
            session_id,
            source_id,
            runtime,
            event,
            payload,
        } => {
            assert_eq!(session_id, 7);
            assert_eq!(source_id, "hook-claude-7");
            assert_eq!(runtime, "claude");
            assert_eq!(event, "Stop");
            assert_eq!(payload, None);
        }
        other => panic!("decoded wrong variant: {other:?}"),
    }

    let event_with_payload = ClientMsg::ReportRuntimeEvent {
        session_id: 8,
        source_id: "hook-claude-8".to_owned(),
        runtime: "claude".to_owned(),
        event: "Stop".to_owned(),
        payload: Some("{\"result\":\"ok\"}".to_owned()),
    };
    let event_with_payload_wire = serde_json::to_value(&event_with_payload).unwrap();
    assert_eq!(
        event_with_payload_wire,
        serde_json::json!({
            "type": "report_runtime_event",
            "session_id": 8,
            "source_id": "hook-claude-8",
            "runtime": "claude",
            "event": "Stop",
            "payload": "{\"result\":\"ok\"}"
        })
    );
    match serde_json::from_value::<ClientMsg>(event_with_payload_wire).unwrap() {
        ClientMsg::ReportRuntimeEvent {
            session_id,
            source_id,
            runtime,
            event,
            payload,
        } => {
            assert_eq!(session_id, 8);
            assert_eq!(source_id, "hook-claude-8");
            assert_eq!(runtime, "claude");
            assert_eq!(event, "Stop");
            assert_eq!(payload.as_deref(), Some("{\"result\":\"ok\"}"));
        }
        other => panic!("decoded wrong variant: {other:?}"),
    }

    let status_capture = ClientMsg::StatusCapture { session_id: 3 };
    let status_capture_wire = serde_json::to_value(&status_capture).unwrap();
    assert_eq!(
        status_capture_wire,
        serde_json::json!({"type": "status_capture", "session_id": 3})
    );
    assert!(matches!(
        serde_json::from_value::<ClientMsg>(status_capture_wire).unwrap(),
        ClientMsg::StatusCapture { session_id: 3 }
    ));

    let ack_wire = serde_json::to_value(ServerMsg::Ack).unwrap();
    assert_eq!(ack_wire, serde_json::json!({"type": "ack"}));
    assert!(matches!(
        serde_json::from_value::<ServerMsg>(ack_wire).unwrap(),
        ServerMsg::Ack
    ));

    let session_send = ClientMsg::SessionSend {
        session: 3,
        text: "ship it\r".to_owned(),
    };
    let session_send_wire = serde_json::to_string(&session_send).unwrap();
    assert_eq!(
        session_send_wire,
        r#"{"type":"session_send","session":3,"text":"ship it\r"}"#
    );
    match serde_json::from_str::<ClientMsg>(&session_send_wire).unwrap() {
        ClientMsg::SessionSend { session, text } => {
            assert_eq!(session, 3);
            assert_eq!(text, "ship it\r");
        }
        other => panic!("decoded wrong variant: {other:?}"),
    }
    assert_eq!(
        session_send.rpc_method(),
        "jackin.capsule.Control/SessionSend"
    );
    assert!(!session_send.is_subscription());
}

#[test]
fn events_subscription_and_session_replies_keep_their_public_contracts() {
    let events = ClientMsg::Events { session: None };
    let events_wire = serde_json::to_value(&events).unwrap();
    assert_eq!(events_wire, serde_json::json!({"type": "events"}));
    assert_eq!(events.rpc_method(), "jackin.capsule.Control/Events");
    assert!(matches!(
        serde_json::from_value::<ClientMsg>(events_wire).unwrap(),
        ClientMsg::Events { session: None }
    ));
    let scoped_events = ClientMsg::Events { session: Some(4) };
    let scoped_events_wire = serde_json::to_value(&scoped_events).unwrap();
    assert_eq!(
        scoped_events_wire,
        serde_json::json!({"type": "events", "session": 4})
    );
    assert!(matches!(
        serde_json::from_value::<ClientMsg>(scoped_events_wire).unwrap(),
        ClientMsg::Events { session: Some(4) }
    ));

    let sent = ServerMsg::SessionSent {
        session: 3,
        bytes: 8,
    };
    let sent_wire = serde_json::to_value(&sent).unwrap();
    assert_eq!(
        sent_wire,
        serde_json::json!({"type": "session_sent", "session": 3, "bytes": 8})
    );
    match serde_json::from_value::<ServerMsg>(sent_wire).unwrap() {
        ServerMsg::SessionSent { session, bytes } => {
            assert_eq!(session, 3);
            assert_eq!(bytes, 8);
        }
        other => panic!("decoded wrong variant: {other:?}"),
    }

    let denied = ServerMsg::SessionSendDenied {
        session: 9,
        reason: SessionSendRejection::UnknownSession,
    };
    let denied_wire = serde_json::to_value(&denied).unwrap();
    assert_eq!(
        denied_wire,
        serde_json::json!({
            "type": "session_send_denied",
            "session": 9,
            "reason": "unknown_session"
        })
    );
    match serde_json::from_value::<ServerMsg>(denied_wire).unwrap() {
        ServerMsg::SessionSendDenied { session, reason } => {
            assert_eq!(session, 9);
            assert_eq!(reason, SessionSendRejection::UnknownSession);
            assert_eq!(reason.label(), "no such session");
        }
        other => panic!("decoded wrong variant: {other:?}"),
    }
    assert!(events.is_subscription());
}

#[test]
fn refreshing_placeholder_requires_unavailable_status_and_empty_usage() {
    let mut refreshing = FocusedUsageView::refreshing(Some("Codex"), 0);
    assert!(refreshing.is_refreshing_placeholder());
    refreshing.focused_agent = Some("codex".to_owned());
    refreshing.focused_provider = Some("OpenAI / Codex".to_owned());
    refreshing.account.provider_label = "Codex CLI".to_owned();
    assert!(refreshing.is_refreshing_placeholder());

    for status in [
        UsageSnapshotStatus::Fresh,
        UsageSnapshotStatus::Stale,
        UsageSnapshotStatus::Error,
    ] {
        let mut view = refreshing.clone();
        view.status = status;
        assert!(!view.is_refreshing_placeholder());
    }

    let mut with_bucket = refreshing.clone();
    with_bucket.buckets.push(placeholder_sample_bucket());
    assert!(!with_bucket.is_refreshing_placeholder());

    let mut with_account = refreshing.clone();
    with_account.account.account_label = "Account".to_owned();
    assert!(!with_account.is_refreshing_placeholder());

    let mut status_bar_only = FocusedUsageView::unavailable("not refreshing", 0);
    status_bar_only.status_bar_label = "refreshing".to_owned();
    assert!(!status_bar_only.is_refreshing_placeholder());

    let mut updated_only = FocusedUsageView::unavailable("not refreshing", 0);
    updated_only.updated_label = "Refreshing".to_owned();
    assert!(!updated_only.is_refreshing_placeholder());

    let error_only = FocusedUsageView::unavailable("refreshing", 0);
    assert!(!error_only.is_refreshing_placeholder());
}
