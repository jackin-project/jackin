// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn boundary_count_account() -> AccountUsageSnapshotView {
    let mut quota_bucket = placeholder_sample_bucket();
    quota_bucket.count_quota = Some(CountQuota::new(
        Some(u64::MAX),
        Some(u64::MAX),
        Some(0),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));

    AccountUsageSnapshotView {
        provider: "Codex".to_owned(),
        account_label: "alexey@example.com".to_owned(),
        account_capability: None,
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
    }
}

#[test]
fn usage_messages_roundtrip_public_wire_values() {
    let focused = FocusedUsageView::unavailable("no focused agent session", 123);
    let focused_wire = serde_json::to_value(&ServerMsg::UsageFocused {
        usage: Box::new(focused),
    })
    .unwrap();
    assert_eq!(focused_wire["type"], "usage_focused");
    assert_eq!(focused_wire["usage"]["status"], "unavailable");
    assert_eq!(focused_wire["usage"]["fetched_at_epoch"], 123);
    match serde_json::from_value::<ServerMsg>(focused_wire).unwrap() {
        ServerMsg::UsageFocused { usage } => {
            assert_eq!(usage.status, UsageSnapshotStatus::Unavailable);
            assert_eq!(usage.fetched_at_epoch, 123);
        }
        other => panic!("unexpected variant {other:?}"),
    }

    let tab = UsageProviderTab {
        id: "sha256:abc".to_owned(),
        label: "Anthropic".to_owned(),
        status_label: "fresh".to_owned(),
        account_label: "a@example.com".to_owned(),
        plan_label: None,
        source_label: None,
        active: true,
        account_capability: None,
    };
    let tab_wire = serde_json::to_value(&tab).unwrap();
    assert_eq!(tab_wire["id"], "sha256:abc");
    assert_eq!(
        serde_json::from_value::<UsageProviderTab>(tab_wire).unwrap(),
        tab
    );
    serde_json::from_str::<UsageProviderTab>(
        r#"{"label":"Anthropic","status_label":"fresh","account_label":"a@example.com","plan_label":null,"source_label":null,"active":true}"#,
    )
    .unwrap_err();

    let request_wire = serde_json::to_value(&ClientMsg::TokenUsage { session_id: 9 }).unwrap();
    assert_eq!(request_wire["type"], "token_usage");
    assert_eq!(request_wire["session_id"], 9);
    assert!(matches!(
        serde_json::from_value::<ClientMsg>(request_wire).unwrap(),
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
    let summary_wire = serde_json::to_value(&ServerMsg::TokenUsage {
        summary: Some(summary.clone()),
    })
    .unwrap();
    assert_eq!(summary_wire["type"], "token_usage");
    assert_eq!(summary_wire["summary"]["input_tokens"], 100);
    assert_eq!(summary_wire["summary"]["output_tokens"], 40);
    assert_eq!(summary_wire["summary"]["cache_read_tokens"], 10);
    assert_eq!(summary_wire["summary"]["cache_write_tokens"], 5);
    assert_eq!(summary_wire["summary"]["cost_usd"], 0.25);
    assert_eq!(summary_wire["summary"]["model"], "claude-opus-4-8");
    match serde_json::from_value::<ServerMsg>(summary_wire).unwrap() {
        ServerMsg::TokenUsage {
            summary: Some(decoded),
        } => assert_eq!(decoded, summary),
        other => panic!("unexpected variant {other:?}"),
    }

    let absent_wire = serde_json::to_value(&ServerMsg::TokenUsage { summary: None }).unwrap();
    assert!(absent_wire["summary"].is_null());
    assert!(matches!(
        serde_json::from_value::<ServerMsg>(absent_wire).unwrap(),
        ServerMsg::TokenUsage { summary: None }
    ));

    let accounts = vec![boundary_count_account()];
    let accounts_wire = serde_json::to_value(&ServerMsg::UsageAccounts {
        accounts: accounts.clone(),
    })
    .unwrap();
    assert_eq!(accounts_wire["type"], "usage_accounts");
    assert_eq!(
        accounts_wire["accounts"][0]["quota_bucket"]["count_quota"]["used"],
        serde_json::json!(u64::MAX)
    );
    assert_eq!(
        accounts_wire["accounts"][0]["quota_bucket"]["count_quota"]["limit"],
        serde_json::json!(u64::MAX)
    );
    assert_eq!(
        accounts_wire["accounts"][0]["quota_bucket"]["count_quota"]["remaining"],
        0
    );
    match serde_json::from_value::<ServerMsg>(accounts_wire).unwrap() {
        ServerMsg::UsageAccounts { accounts: decoded } => assert_eq!(decoded, accounts),
        other => panic!("unexpected variant {other:?}"),
    }
}

#[test]
fn provider_state_and_daily_slot_keep_wire_vocabulary() {
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

    let wire = serde_json::to_value(&bucket).unwrap();
    assert_eq!(wire["provider_quota_state"], "exhausted");
    assert_eq!(wire["count_quota"]["remaining"], 1);
    assert_eq!(
        serde_json::from_value::<QuotaBucketView>(wire.clone()).unwrap(),
        bucket
    );

    let mut absent_state_wire = wire;
    absent_state_wire
        .as_object_mut()
        .unwrap()
        .remove("provider_quota_state");
    let absent_state: QuotaBucketView = serde_json::from_value(absent_state_wire).unwrap();
    assert_eq!(absent_state.provider_quota_state, None);
    let no_state_wire = serde_json::to_value(placeholder_sample_bucket()).unwrap();
    assert!(no_state_wire.get("provider_quota_state").is_none());

    for invalid in ["available", "unknown", "EXHAUSTED", "future"] {
        serde_json::from_value::<ProviderQuotaState>(serde_json::json!(invalid))
            .expect_err("only explicit exhausted or unlimited claims are valid");
    }

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
fn usage_detail_meter_requires_and_serializes_its_side() {
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
    assert_eq!(serde_json::to_value(valid).unwrap()["meter_side"], "used");
}
