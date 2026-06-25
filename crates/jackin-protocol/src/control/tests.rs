//! Tests for `control`.
use super::*;

#[test]
fn client_msg_unknown_decodes_from_unrecognised_tag() {
    let m: ClientMsg = serde_json::from_str(r#"{"type":"future_query"}"#)
        .expect("decode unknown ClientMsg variant");
    assert!(matches!(m, ClientMsg::Unknown));
}

#[test]
fn server_msg_unknown_decodes_from_unrecognised_tag() {
    let m: ServerMsg = serde_json::from_str(r#"{"type":"future_reply"}"#)
        .expect("decode unknown ServerMsg variant");
    assert!(matches!(m, ServerMsg::Unknown));
}

#[test]
fn missing_tag_field_still_bails() {
    // Structural malformations (no `type` key, non-string tag) are
    // not absorbed by `#[serde(other)]` — peers must still emit
    // well-formed tagged JSON.
    assert!(serde_json::from_str::<ClientMsg>(r#"{"foo":"bar"}"#).is_err());
    assert!(serde_json::from_str::<ServerMsg>(r#"{"type":42}"#).is_err());
}

#[test]
fn known_variants_roundtrip() {
    let json = serde_json::to_string(&ClientMsg::Status).unwrap();
    assert_eq!(json, r#"{"type":"status"}"#);
    let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, ClientMsg::Status));
}

#[test]
fn report_runtime_event_roundtrips_and_omits_none_payload() {
    let msg = ClientMsg::ReportRuntimeEvent {
        session_id: 7,
        source_id: "hook-claude-7".to_owned(),
        runtime: "claude".to_owned(),
        event: "Stop".to_owned(),
        payload: None,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(
        !json.contains("payload"),
        "a None payload must be omitted from the wire: {json}"
    );
    match serde_json::from_str::<ClientMsg>(&json).unwrap() {
        ClientMsg::ReportRuntimeEvent {
            session_id, event, ..
        } => {
            assert_eq!(session_id, 7);
            assert_eq!(event, "Stop");
        }
        other => panic!("decoded wrong variant: {other:?}"),
    }
}

#[test]
fn status_capture_and_ack_roundtrip() {
    let json = serde_json::to_string(&ClientMsg::StatusCapture { session_id: 3 }).unwrap();
    assert!(matches!(
        serde_json::from_str::<ClientMsg>(&json).unwrap(),
        ClientMsg::StatusCapture { session_id: 3 }
    ));
    let ack = serde_json::to_string(&ServerMsg::Ack).unwrap();
    assert!(matches!(
        serde_json::from_str::<ServerMsg>(&ack).unwrap(),
        ServerMsg::Ack
    ));
}
