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
fn usage_focused_roundtrips() {
    let usage = FocusedUsageView::unavailable("no focused agent session", 123);
    let json = serde_json::to_string(&ServerMsg::UsageFocused {
        usage: Box::new(usage.clone()),
    })
    .unwrap();
    let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
    match decoded {
        ServerMsg::UsageFocused { usage: decoded } => {
            assert_eq!(decoded.status, UsageSnapshotStatus::Unavailable);
            assert_eq!(decoded.fetched_at_epoch, 123);
        }
        other => panic!("unexpected variant {other:?}"),
    }
}

#[test]
fn usage_account_list_roundtrips() {
    let accounts = vec![AccountUsageSnapshotView {
        provider: "Codex".to_owned(),
        account_label: "alexey@example.com".to_owned(),
        source: "cli".to_owned(),
        confidence: "authoritative".to_owned(),
        window_kind: "Session".to_owned(),
        used_amount: Some(63),
        used_unit: Some("percent".to_owned()),
        limit_amount: Some(100),
        limit_unit: Some("percent".to_owned()),
        resets_at: Some(1_781_190_720),
        fetched_at: 1_781_185_560,
        expires_at: Some(1_781_185_860),
        status: "fresh".to_owned(),
        last_error: None,
    }];
    let json = serde_json::to_string(&ServerMsg::UsageAccounts {
        accounts: accounts.clone(),
    })
    .unwrap();
    let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
    match decoded {
        ServerMsg::UsageAccounts { accounts: decoded } => assert_eq!(decoded, accounts),
        other => panic!("unexpected variant {other:?}"),
    }
}
