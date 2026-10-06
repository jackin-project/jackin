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
fn usage_provider_tab_id_roundtrips_and_defaults_when_absent() {
    let tab = UsageProviderTab {
        id: "sha256:abc".to_owned(),
        label: "Anthropic".to_owned(),
        status_label: "fresh".to_owned(),
        account_label: "a@example.com".to_owned(),
        plan_label: None,
        source_label: None,
        active: true,
    };
    let decoded: UsageProviderTab =
        serde_json::from_str(&serde_json::to_string(&tab).unwrap()).unwrap();
    assert_eq!(decoded, tab);
    // Tabs persisted before the id field decode with an empty id rather than
    // failing; the producer always stamps real ids.
    let legacy: UsageProviderTab = serde_json::from_str(
        r#"{"label":"Anthropic","status_label":"fresh","account_label":"a@example.com","plan_label":null,"source_label":null,"active":true}"#,
    )
    .unwrap();
    assert_eq!(legacy.id, "");
    assert_eq!(legacy.label, "Anthropic");
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
fn money_raw_percent_keeps_overage_and_rejects_bad_denominations() {
    // Overage survives unclamped; the projection and the capsule presentation
    // share this rule, so both recover the same magnitude.
    assert_eq!(
        Money::new(15_000, "USD", 2).raw_percent_of(&Money::new(10_000, "USD", 2)),
        Some(150)
    );
    assert_eq!(
        Money::new(27_00, "USD", 2).raw_percent_of(&Money::new(30_000, "USD", 2)),
        Some(9)
    );
    assert_eq!(
        Money::new(0, "USD", 2).raw_percent_of(&Money::new(10_000, "USD", 2)),
        Some(0)
    );
    // Incompatible denominations, a non-positive cap, and overflow saturate
    // to no representation instead of a wrapped or fabricated value.
    assert_eq!(
        Money::new(50_00, "USD", 2).raw_percent_of(&Money::new(10_000, "SGD", 2)),
        None
    );
    assert_eq!(
        Money::new(1, "USD", 2).raw_percent_of(&Money::new(0, "USD", 2)),
        None
    );
    assert_eq!(
        Money::new(i64::MAX, "USD", 2).raw_percent_of(&Money::new(1, "USD", 2)),
        Some(i32::MAX)
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

#[test]
fn session_send_roundtrips_text_verbatim() {
    // The submit key is part of `text` — the daemon appends nothing, so a
    // payload carrying `\r` must survive the round trip byte for byte.
    let msg = ClientMsg::SessionSend {
        session: 3,
        text: "ship it\r".to_owned(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    match serde_json::from_str::<ClientMsg>(&json).unwrap() {
        ClientMsg::SessionSend { session, text } => {
            assert_eq!(session, 3);
            assert_eq!(text, "ship it\r");
        }
        other => panic!("decoded wrong variant: {other:?}"),
    }
    assert_eq!(msg.rpc_method(), "jackin.capsule.Control/SessionSend");
    assert!(!msg.is_subscription());
}

#[test]
fn session_send_replies_roundtrip() {
    let json = serde_json::to_string(&ServerMsg::SessionSent {
        session: 3,
        bytes: 8,
    })
    .unwrap();
    match serde_json::from_str::<ServerMsg>(&json).unwrap() {
        ServerMsg::SessionSent { session, bytes } => {
            assert_eq!((session, bytes), (3, 8));
        }
        other => panic!("decoded wrong variant: {other:?}"),
    }

    let json = serde_json::to_string(&ServerMsg::SessionSendDenied {
        session: 9,
        reason: SessionSendRejection::UnknownSession,
    })
    .unwrap();
    match serde_json::from_str::<ServerMsg>(&json).unwrap() {
        ServerMsg::SessionSendDenied { session, reason } => {
            assert_eq!(session, 9);
            assert_eq!(reason, SessionSendRejection::UnknownSession);
            assert_eq!(reason.label(), "no such session");
        }
        other => panic!("decoded wrong variant: {other:?}"),
    }
}

#[test]
fn events_request_omits_absent_filter_and_declares_itself_a_subscription() {
    let all = ClientMsg::Events { session: None };
    let json = serde_json::to_string(&all).unwrap();
    assert_eq!(json, r#"{"type":"events"}"#);
    assert!(all.is_subscription());
    assert_eq!(all.rpc_method(), "jackin.capsule.Control/Events");

    let one = ClientMsg::Events { session: Some(4) };
    let json = serde_json::to_string(&one).unwrap();
    assert!(json.contains(r#""session":4"#), "{json}");
    assert!(matches!(
        serde_json::from_str::<ClientMsg>(&json).unwrap(),
        ClientMsg::Events { session: Some(4) }
    ));
}

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
