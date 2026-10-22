// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
    let legacy: UsageProviderTab = serde_json::from_str(
        r#"{"label":"Anthropic","status_label":"fresh","account_label":"a@example.com","plan_label":null,"source_label":null,"active":true}"#,
    )
    .unwrap();
    assert_eq!(legacy.id, "");
    assert_eq!(legacy.label, "Anthropic");
}

#[test]
fn token_usage_roundtrips_present_and_absent() {
    let json = serde_json::to_string(&ClientMsg::TokenUsage { session_id: 9 }).unwrap();
    assert!(matches!(
        serde_json::from_str::<ClientMsg>(&json).unwrap(),
        ClientMsg::TokenUsage { session_id: 9 }
    ));

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

    let json = serde_json::to_string(&ServerMsg::TokenUsage { summary: None }).unwrap();
    assert!(matches!(
        serde_json::from_str::<ServerMsg>(&json).unwrap(),
        ServerMsg::TokenUsage { summary: None }
    ));
}

#[test]
fn provider_quota_state_is_an_explicit_independent_bucket_observation() {
    let mut bucket = placeholder_sample_bucket();
    bucket.provider_quota_state = Some(ProviderQuotaState::Exhausted);
    bucket.count_quota = Some(CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    ));
    bucket.validate().unwrap();

    let encoded = serde_json::to_value(&bucket).unwrap();
    assert_eq!(encoded["provider_quota_state"], "exhausted");
    assert_eq!(encoded["count_quota"]["remaining"], 1);
    let decoded: QuotaBucketView = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, bucket);

    let mut absent_state = serde_json::to_value(placeholder_sample_bucket()).unwrap();
    absent_state
        .as_object_mut()
        .unwrap()
        .remove("provider_quota_state");
    let decoded: QuotaBucketView = serde_json::from_value(absent_state).unwrap();
    assert_eq!(decoded.provider_quota_state, None);

    for label in ["available", "unknown", "EXHAUSTED", "future"] {
        serde_json::from_value::<ProviderQuotaState>(serde_json::json!(label))
            .expect_err("only affirmative exhausted/unlimited source claims are valid");
    }
}

#[test]
fn usage_account_list_roundtrips() {
    let mut quota_bucket = placeholder_sample_bucket();
    quota_bucket.count_quota = Some(CountQuota::new(
        Some(u64::MAX),
        Some(u64::MAX),
        Some(0),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
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
        quota_bucket: Some(quota_bucket),
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
fn quota_bucket_keeps_percent_origins_counts_and_money_separate() {
    let mut bucket = placeholder_sample_bucket();
    bucket.used_percent = Some(CanonicalUsedPercent::new(43.375).unwrap());
    bucket.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    bucket.count_quota = Some(CountQuota::new(
        Some(u64::MAX),
        None,
        Some(0),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    ));
    bucket.validate().unwrap();

    let json = serde_json::to_string(&bucket).unwrap();
    let decoded: QuotaBucketView = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, bucket);
    assert!(json.contains("18446744073709551615"));
    assert!(json.contains("43.375"));

    bucket.used_percent_origin = None;
    assert!(bucket.validate().unwrap_err().contains("origin"));
    bucket.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    bucket.used_money = Some(Money::new(-1, "USD", 2));
    assert!(bucket.validate().unwrap_err().contains("count and money"));
}

#[test]
fn usage_detail_meter_requires_a_side() {
    let row = UsageDetailRow {
        row_id: "weekly".to_owned(),
        kind: UsageDetailRowKind::Bucket,
        label: "Weekly".to_owned(),
        layout_lines: Vec::new(),
        display_label: "43.38% used".to_owned(),
        meter_percent: Some(MeterPercentPoints::new(43.375).unwrap()),
        meter_side: None,
        severity: UsageSeverity::Normal,
    };
    assert!(row.validate().unwrap_err().contains("side"));
    let valid = UsageDetailRow {
        meter_side: Some(UsageMeterSide::Used),
        ..row
    };
    valid.validate().unwrap();
}

#[test]
fn money_scales_minor_units_by_exponent() {
    let usd = Money::new(5331, "USD", 2);
    assert!((usd.major() - 53.31).abs() < 1e-9);
    assert_eq!(usd.to_string(), "$53.31");
    assert_eq!(usd.format_compact(), "$53");
}

#[test]
fn money_formats_currency_and_credit_labels() {
    assert_eq!(Money::new(7849, "SGD", 2).to_string(), "SGD 78.49");
    assert_eq!(Money::new(7849, "SGD", 2).format_compact(), "SGD 78");
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
    assert_eq!(
        serde_json::to_value(StatusSlot::Daily).unwrap(),
        serde_json::json!("daily")
    );
    assert_eq!(
        serde_json::from_value::<StatusSlot>(serde_json::json!("daily")).unwrap(),
        StatusSlot::Daily
    );
}

#[test]
fn refreshing_placeholder_accepts_constructor_and_surface_decoration() {
    let mut view = FocusedUsageView::refreshing(Some("Codex"), 0);
    assert!(view.is_refreshing_placeholder());
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
fn operator_ownership_messages_roundtrip() {
    for msg in [
        ClientMsg::TakeControl { session: 3 },
        ClientMsg::ReleaseControl { session: 3 },
    ] {
        let json = serde_json::to_string(&msg).unwrap();
        let decoded = serde_json::from_str::<ClientMsg>(&json).unwrap();
        assert!(
            matches!(
                (&msg, &decoded),
                (
                    ClientMsg::TakeControl { session: 3 },
                    ClientMsg::TakeControl { session: 3 }
                ) | (
                    ClientMsg::ReleaseControl { session: 3 },
                    ClientMsg::ReleaseControl { session: 3 }
                )
            ),
            "ownership message must roundtrip, got {decoded:?}"
        );
        assert!(!decoded.is_subscription());
    }
    assert_eq!(
        ClientMsg::TakeControl { session: 3 }.rpc_method(),
        "jackin.capsule.Control/TakeControl"
    );
    assert_eq!(
        ClientMsg::ReleaseControl { session: 3 }.rpc_method(),
        "jackin.capsule.Control/ReleaseControl"
    );
    assert_eq!(
        SessionSendRejection::OperatorOwned.label(),
        "session input is owned by an operator"
    );
    let json = serde_json::to_string(&ServerMsg::SessionSendDenied {
        session: 9,
        reason: SessionSendRejection::OperatorOwned,
    })
    .unwrap();
    match serde_json::from_str::<ServerMsg>(&json).unwrap() {
        ServerMsg::SessionSendDenied { session, reason } => {
            assert_eq!(session, 9);
            assert_eq!(reason, SessionSendRejection::OperatorOwned);
        }
        other => panic!("decoded wrong variant: {other:?}"),
    }
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
