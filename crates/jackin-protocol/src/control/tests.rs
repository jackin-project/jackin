// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
    serde_json::from_str::<ClientMsg>(r#"{"foo":"bar"}"#).unwrap_err();
    serde_json::from_str::<ServerMsg>(r#"{"type":42}"#).unwrap_err();
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
fn token_usage_roundtrips_present_and_absent() {
    // Request side.
    let json = serde_json::to_string(&ClientMsg::TokenUsage { session_id: 9 }).unwrap();
    assert!(matches!(
        serde_json::from_str::<ClientMsg>(&json).unwrap(),
        ClientMsg::TokenUsage { session_id: 9 }
    ));

    // Reply with a summary.
    let summary = TokenUsageSummary {
        input_tokens: 100,
        output_tokens: 40,
        cache_read_tokens: 10,
        cache_write_tokens: 5,
        cost_usd: Some(0.25),
        model: Some("claude-opus-4-8".to_owned()),
    };
    let json = serde_json::to_string(&ServerMsg::TokenUsage {
        summary: Some(summary.clone()),
    })
    .unwrap();
    match serde_json::from_str::<ServerMsg>(&json).unwrap() {
        ServerMsg::TokenUsage { summary: Some(s) } => assert_eq!(s, summary),
        other => panic!("unexpected variant {other:?}"),
    }

    // Reply for an unknown session.
    let json = serde_json::to_string(&ServerMsg::TokenUsage { summary: None }).unwrap();
    assert!(matches!(
        serde_json::from_str::<ServerMsg>(&json).unwrap(),
        ServerMsg::TokenUsage { summary: None }
    ));
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

#[test]
fn money_scales_minor_units_by_exponent() {
    // 5331 minor @ exponent 2 = 53.31 major — the value that, mis-scaled as
    // major units, produced the 100×-too-large spend bug.
    let usd = Money::new(5331, "USD", 2);
    assert!((usd.major() - 53.31).abs() < 1e-9);
    assert_eq!(usd.to_string(), "$53.31");
    assert_eq!(usd.format_compact(), "$53");
}

#[test]
fn money_formats_currency_and_credit_labels() {
    // ISO-4217 non-USD code: leading code, full precision vs compact.
    assert_eq!(Money::new(7849, "SGD", 2).to_string(), "SGD 78.49");
    assert_eq!(Money::new(7849, "SGD", 2).format_compact(), "SGD 78");
    // Non-standard label (credits) renders the unit as a suffix.
    assert_eq!(
        Money::new(30000, "credits", 2).to_string(),
        "300.00 credits"
    );
    assert_eq!(
        Money::new(30000, "credits", 2).format_compact(),
        "300 credits"
    );
}

#[test]
fn status_slot_daily_serializes_as_daily() {
    // snake_case serde repr, matching the FFI `"daily"` slot projection.
    assert_eq!(
        serde_json::to_value(StatusSlot::Daily).unwrap(),
        serde_json::json!("daily")
    );
    assert_eq!(
        serde_json::from_value::<StatusSlot>(serde_json::json!("daily")).unwrap(),
        StatusSlot::Daily
    );
}

fn placeholder_sample_bucket() -> QuotaBucketView {
    QuotaBucketView {
        label: "Weekly".to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(57),
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Weekly),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}

#[test]
fn refreshing_placeholder_accepts_constructor_and_surface_decoration() {
    let mut view = FocusedUsageView::refreshing(Some("Codex"), 0);
    assert!(view.is_refreshing_placeholder());
    // Surface decoration (provider/agent/tabs/provider_label) is allowed.
    view.focused_agent = Some("codex".to_owned());
    view.focused_provider = Some("OpenAI / Codex".to_owned());
    view.account.provider_label = "OpenAI / Codex".to_owned();
    assert!(view.is_refreshing_placeholder());
}

#[test]
fn refreshing_placeholder_rejects_state_and_string_lookalikes() {
    let base = FocusedUsageView::refreshing(Some("Codex"), 0);

    let mut fresh = base.clone();
    fresh.status = UsageSnapshotStatus::Fresh;
    let mut stale = base.clone();
    stale.status = UsageSnapshotStatus::Stale;
    let mut error = base.clone();
    error.status = UsageSnapshotStatus::Error;
    let mut with_bucket = base.clone();
    with_bucket.buckets.push(placeholder_sample_bucket());
    let mut with_account = base.clone();
    with_account.account.account_label = "user@example.com".to_owned();
    // Single-string lookalikes: a non-placeholder view matching only one field.
    let mut only_bar = FocusedUsageView::unavailable("other", 0);
    only_bar.status_bar_label = "refreshing".to_owned();
    let mut only_updated = FocusedUsageView::unavailable("other", 0);
    only_updated.updated_label = "Refreshing".to_owned();
    let mut only_error = FocusedUsageView::unavailable("other", 0);
    only_error.last_error = Some("refreshing".to_owned());

    for view in [
        fresh,
        stale,
        error,
        with_bucket,
        with_account,
        only_bar,
        only_updated,
        only_error,
    ] {
        assert!(!view.is_refreshing_placeholder());
    }
}
