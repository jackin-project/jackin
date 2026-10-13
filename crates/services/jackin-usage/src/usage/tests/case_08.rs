// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn grok_billing_config_maps_current_fallback_and_bounds() {
    let usage: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "subscription_tier": "SuperGrok",
        "on_demand_enabled": true,
        "config": {
            "monthlyLimit": { "val": 5000 },
            "used": { "val": 1800 },
            "billingPeriodStart": "2026-06-01T00:00:00Z",
            "billingPeriodEnd": "2026-07-01T00:00:00Z",
            "prepaidBalance": { "val": 2500 },
            "onDemandCap": { "val": 4000 },
            "onDemandUsed": { "val": 300 }
        }
    }))
    .expect("valid current Grok billing response");

    assert_eq!(usage.plan_label().as_deref(), Some("SuperGrok"));
    let buckets = usage.buckets(1_780_315_200);

    // One headline (Weekly slot), detail rows stay untagged.
    assert_eq!(buckets[0].label, "Monthly");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[0].remaining_percent, Some(64));
    assert!(
        buckets[1..]
            .iter()
            .all(|bucket| bucket.status_slot.is_none())
    );

    let credits = buckets
        .iter()
        .find(|bucket| bucket.label == "Extra usage credits")
        .expect("prepaid balance bound");
    assert_eq!(credits.limit_label.as_deref(), Some("$25"));
    assert!(credits.limit_money.is_some());
    assert_eq!(credits.used_label, None);

    let on_demand = buckets
        .iter()
        .find(|bucket| bucket.label == "On-demand usage")
        .expect("on-demand bound");
    assert_eq!(on_demand.used_label.as_deref(), Some("$3"));
    assert_eq!(on_demand.limit_label.as_deref(), Some("$40"));
    assert!(on_demand.limit_money.is_some());
}

#[test]
fn grok_billing_config_preferred_percent_path_has_pace() {
    let usage: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "config": {
            "creditUsagePercent": 43.0,
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-06-01T00:00:00Z",
                "end": "2026-06-08T00:00:00Z"
            }
        }
    }))
    .expect("valid preferred config");
    let now = parse_iso_epoch("2026-06-04T00:00:00Z").expect("now");
    let buckets = usage.buckets(now);
    assert_eq!(buckets[0].label, "Weekly");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[0].remaining_percent, Some(57));
    assert!(buckets[0].pace_label.is_some());
}

#[test]
fn grok_plan_label_from_server_tier_only() {
    let free: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "config": {},
        "subscription_tier": "  "
    }))
    .expect("blank tier");
    assert_eq!(free.plan_label(), None);
    // Web path never guesses a plan (no auth heuristic).
    let web = GrokBillingSnapshot::Web(GrokWebBillingSnapshot {
        used_percent: 40.0,
        reset_at_epoch: Some(1_780_315_200),
    });
    assert_eq!(web.plan_label(), None);
}

#[test]
fn grok_on_demand_requires_positive_cap() {
    let usage: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "on_demand_enabled": true,
        "config": { "onDemandUsed": { "val": 500 } }
    }))
    .expect("no cap config");
    let buckets = usage.buckets(1_780_315_200);
    // Used without a positive provider cap is unbounded spend, not a quota bound.
    assert!(
        buckets
            .iter()
            .all(|bucket| bucket.label != "On-demand usage")
    );
}

#[test]
fn grok_rpc_payload_keeps_billing_method_unescaped() {
    let payload = grok_rpc_request_payload(2, "x.ai/billing", serde_json::json!({}));
    let encoded = serde_json::to_string(&payload).expect("encode payload");

    assert!(encoded.contains("\"method\":\"x.ai/billing\""));
    assert!(!encoded.contains("x.ai\\/billing"));
}

#[test]
fn grok_account_label_prefers_auth_identity_over_env_presence() {
    let dir = tempfile::tempdir().expect("tempdir");
    let auth = dir.path().join("auth.json");
    fs::write(
        &auth,
        r#"{"account":{"email":"operator@example.com"},"token":"redacted"}"#,
    )
    .expect("write auth");

    let label = grok_account_label_or_presence(&auth, true, true, true);

    assert_eq!(label, "operator@example.com");
}

#[test]
fn grok_account_label_reports_safe_credential_presence() {
    let missing = Path::new("/tmp/nonexistent-grok-auth-for-test.json");

    assert_eq!(
        grok_account_label_or_presence(missing, false, true, true),
        "XAI_API_KEY present"
    );
    assert_eq!(
        grok_account_label_or_presence(missing, false, false, true),
        "GROK_DEPLOYMENT_KEY present"
    );
    assert_eq!(
        grok_account_label_or_presence(missing, false, false, false),
        "needs Grok login"
    );
}

#[test]
fn grok_snapshot_uses_probe_success_without_local_credential_marker() {
    let missing = Path::new("/tmp/nonexistent-grok-auth-for-test.json");
    let billing: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "config": {
            "monthlyLimit": { "val": 5000 },
            "used": { "val": 1000 },
            "billingPeriodStart": "2026-06-01T00:00:00Z",
            "billingPeriodEnd": "2026-07-01T00:00:00Z"
        }
    }))
    .expect("valid current Grok billing response");

    let view = grok_snapshot_from_rpc_result(
        "grok",
        1_780_315_200,
        missing,
        false,
        false,
        false,
        Ok(GrokBillingSnapshot::Rpc(Box::new(billing))),
    );

    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.source, UsageSource::Cli);
    assert_eq!(view.confidence, UsageConfidence::Authoritative);
    assert_eq!(view.account.account_label, "needs Grok login");
    assert_eq!(view.buckets[0].label, "Monthly");
    assert_eq!(view.buckets[0].remaining_percent, Some(80));
    assert_eq!(view.last_error, None);
}

#[test]
fn grok_web_billing_response_maps_weekly_usage() {
    let data = [
        0x00, 0x00, 0x00, 0x00, 0x3c, 0x0a, 0x3a, 0x0d, 0x9c, 0x7d, 0xac, 0x42, 0x12, 0x00, 0x1a,
        0x00, 0x22, 0x06, 0x08, 0x80, 0x97, 0xf3, 0xd0, 0x06, 0x2a, 0x06, 0x08, 0x80, 0xb1, 0x91,
        0xd2, 0x06, 0x3a, 0x07, 0x08, 0x02, 0x15, 0x12, 0x03, 0xa5, 0x42, 0x42, 0x12, 0x08, 0x01,
        0x12, 0x06, 0x08, 0x80, 0x97, 0xf3, 0xd0, 0x06, 0x1a, 0x06, 0x08, 0x80, 0xb1, 0x91, 0xd2,
        0x06, 0x62, 0x00, 0x68, 0x01, 0x72, 0x00, 0x7a, 0x00, 0x82, 0x01, 0x00, 0x8a, 0x01, 0x00,
        0x92, 0x01, 0x00, 0x9a, 0x01, 0x00, 0xa2, 0x01, 0x00, 0xaa, 0x01, 0x00,
    ];

    let snapshot =
        parse_grok_web_billing_response(&data, 1_782_318_000).expect("parse grok billing");
    let buckets = snapshot.buckets(1_782_318_000);
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Weekly));

    assert_eq!(buckets[0].label, "Weekly");
    assert_eq!(buckets[0].remaining_percent, Some(14));
    assert_eq!(
        buckets[0].reset_label.as_deref(),
        Some(
            reset_label(
                parse_iso_epoch("2026-07-01T00:00:00Z").expect("billing reset"),
                1_782_318_000,
            )
            .as_str()
        )
    );
}

#[test]
fn grok_cycle_label_falls_back_to_credits_for_irregular_cycles() {
    assert_eq!(grok_cycle_label_from_minutes(7 * 24 * 60), "Weekly");
    assert_eq!(grok_cycle_label_from_minutes(30 * 24 * 60), "Monthly");
    assert_eq!(grok_cycle_label_from_minutes(13 * 24 * 60), "Credits");
}

#[test]
fn grok_snapshot_reports_probe_error_instead_of_presence_gate() {
    let missing = Path::new("/tmp/nonexistent-grok-auth-for-test.json");

    let view = grok_snapshot_from_rpc_result(
        "grok",
        1_780_315_200,
        missing,
        false,
        false,
        false,
        Err(ProviderError::from(
            "grok agent stdio failed to start: not found".to_owned(),
        )),
    );

    assert_eq!(view.status, UsageSnapshotStatus::NeedsLogin);
    assert_eq!(view.source, UsageSource::None);
    assert_eq!(view.confidence, UsageConfidence::None);
    assert_eq!(
        view.last_error.as_deref(),
        Some("grok agent stdio failed to start: not found")
    );
}

#[test]
fn codex_oauth_credentials_parse_nested_tokens() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("auth.json");
    let id_token = test_jwt(serde_json::json!({
        "email": "person@example.com",
        "sub": "acct-sub"
    }));
    fs::write(
        &path,
        serde_json::json!({
            "tokens": {
                "access_token": "access",
                "refresh_token": "refresh",
                "account_id": "acct",
                "id_token": id_token
            }
        })
        .to_string(),
    )
    .expect("write auth");

    let credentials = load_codex_oauth_credentials(&path).expect("credentials");

    assert_eq!(credentials.access_token, "access");
    assert_eq!(credentials.account_id.as_deref(), Some("acct"));
    assert_eq!(
        credentials.account_label.as_deref(),
        Some("person@example.com")
    );
}

#[test]
fn codex_id_token_identity_falls_back_to_subject() {
    let id_token = test_jwt(serde_json::json!({
        "sub": "user-123"
    }));

    assert_eq!(
        codex_account_label_from_id_token(&id_token).as_deref(),
        Some("ChatGPT account user-123")
    );
}
