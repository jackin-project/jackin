// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

const PERIOD_FIXTURE: &str = r#"{
    "usage": {
        "enabled": true,
        "planUsage": {"limit": 20, "totalPercentUsed": 34, "totalSpend": 6.8},
        "spendLimitUsage": {"limitType": "personal", "pooledLimit": 0}
    }
}"#;

const SUMMARY_FIXTURE: &str = r#"{
    "billingCycleStart": "2026-09-01T00:00:00Z",
    "billingCycleEnd": "2026-10-01T00:00:00Z",
    "membershipType": "pro",
    "individualUsage": {
        "plan": {"totalPercentUsed": 41, "autoPercentUsed": 12, "apiPercentUsed": 3},
        "onDemand": 4.25,
        "overall": 77
    },
    "teamUsage": {"onDemand": 1.5, "pooled": 9.75}
}"#;

#[test]
fn jwt_user_id_derives_after_pipe() {
    // Sanitized fixture token: header + {"sub": "auth0|user_123"} + sig.
    let token = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhdXRoMHx1c2VyXzEyMyJ9.c2lnbmF0dXJl";
    assert_eq!(
        cursor_user_id_from_token(token).as_deref(),
        Some("user_123")
    );
    assert_eq!(cursor_user_id_from_token("opaque-token"), None);
    assert_eq!(cursor_user_id_from_token(""), None);
}

#[test]
fn session_cookie_shape() {
    assert_eq!(
        cursor_session_cookie("user_123", "tok"),
        "WorkosCursorSessionToken=user_123%3A%3Atok"
    );
}

#[test]
fn scope_urls_never_cross() {
    // Hermetic: the pure join, not the env-reading wrapper, so a live
    // `CURSOR_API_ENDPOINT` can neither break nor leak into this test.
    assert!(
        cursor_dashboard_url_with_base(CURSOR_DEFAULT_DASHBOARD_BASE, "GetPlanInfo")
            .starts_with("https://api2.cursor.sh/")
    );
    assert!(
        cursor_dashboard_url_with_base("https://proxy.test/", "GetPlanInfo")
            .starts_with("https://proxy.test/aiserver.v1.DashboardService/GetPlanInfo")
    );
    assert!(!cursor_teams_spend_url().contains("api2.cursor.sh"));
    assert!(!cursor_teams_events_url().contains("api2.cursor.sh"));
    assert!(cursor_teams_spend_url().starts_with("https://api.cursor.com/"));
}

#[test]
fn period_parses_with_team_inference() {
    let usage = parse_cursor_period_usage(&serde_json::from_str(PERIOD_FIXTURE).expect("json"))
        .expect("period parses");
    assert!(usage.enabled);
    assert!(!usage.is_team);
    assert!(!cursor_needs_request_fallback(&usage));
    let buckets = cursor_period_buckets(&usage, None, 1_781_728_000);
    assert_eq!(buckets.len(), 2);
    assert_eq!(buckets[0].label, "Billing cycle");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[0].remaining_percent, Some(66));
    assert_eq!(buckets[1].label, "Spend (actual)");
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Spend));
    // Spend scale is unverified: labels only, no structured money.
    assert!(buckets[1].used_money.is_none());

    let team = serde_json::json!({
        "usage": {"planUsage": {"totalPercentUsed": 10},
                  "spendLimitUsage": {"limitType": "team", "pooledLimit": 500}}
    });
    let usage = parse_cursor_period_usage(&team).expect("team parses");
    assert!(usage.is_team);
    assert!(cursor_needs_request_fallback(&usage));
}

#[test]
fn period_parses_top_level_live_shape() {
    // Shape observed live 2026-09-21: `planUsage` at top level, no `usage`
    // wrapper (usage counters rounded; no credentials involved).
    let live = serde_json::json!({
        "billingCycleStart": "1789542030000",
        "billingCycleEnd": "1792134030000",
        "planUsage": {
            "totalSpend": 35950,
            "includedSpend": 35950,
            "remaining": 4050,
            "limit": 40000,
            "autoPercentUsed": 10.289,
            "apiPercentUsed": 50.82,
            "totalPercentUsed": 11.597
        },
        "spendLimitUsage": {"limitType": "user"},
        "enabled": true
    });
    let usage = parse_cursor_period_usage(&live).expect("live shape parses");
    assert!(usage.enabled);
    assert!(!usage.is_team);
    assert_eq!(usage.limit, Some(40000.0));
    assert_eq!(usage.total_spend, Some(35950.0));
    let buckets = cursor_period_buckets(&usage, None, 1_781_728_000);
    assert_eq!(buckets[0].remaining_percent, Some(88));
}

#[test]
fn profile_auth_and_identity_parse_from_values() {
    let auth = cursor_auth_from_value(&serde_json::json!({"accessToken": "tok"}))
        .expect("camelCase token");
    assert_eq!(auth.access_token, "tok");
    let auth = cursor_auth_from_value(&serde_json::json!({"access_token": "tok2"}))
        .expect("snake_case token");
    assert_eq!(auth.access_token, "tok2");
    assert!(cursor_auth_from_value(&serde_json::json!({"accessToken": "  "})).is_none());
    assert!(cursor_auth_from_value(&serde_json::json!({})).is_none());
    assert_eq!(
        cursor_identity_from_cli_config(
            &serde_json::json!({"authInfo": {"email": "a@test", "displayName": "A"}})
        )
        .as_deref(),
        Some("a@test")
    );
    assert_eq!(
        cursor_identity_from_cli_config(&serde_json::json!({"authInfo": {"displayName": "A"}}))
            .as_deref(),
        Some("A")
    );
    assert_eq!(
        cursor_identity_from_cli_config(&serde_json::json!({})),
        None
    );
}

#[test]
fn profile_snapshot_missing_auth_needs_secret_without_network() {
    let temp = tempfile::tempdir().unwrap();
    let view = cursor_profile_snapshot("cursor", &temp.path().join("auth.json"), 1_781_728_000);
    assert_eq!(view.status, UsageSnapshotStatus::NeedsSecret);
}

#[test]
fn plan_and_grants_parse() {
    assert_eq!(
        parse_cursor_plan_info(&serde_json::json!({"planName": "pro_plus"})),
        Some("Pro Plus".to_owned())
    );
    assert_eq!(
        parse_cursor_credit_grants(&serde_json::json!({"grantTotal": 1500})),
        1500
    );
    assert_eq!(
        parse_cursor_credit_grants(
            &serde_json::json!({"grants": [{"amountCents": 100}, {"amountCents": 50}]})
        ),
        150
    );
    // Total + breakdown: the itemized sum wins, never total + sum.
    assert_eq!(
        parse_cursor_credit_grants(&serde_json::json!({
            "grantTotal": 150,
            "grants": [{"amountCents": 100}, {"amountCents": 50}]
        })),
        150
    );
    // Empty `grants[]` falls back to the top-level total.
    assert_eq!(
        parse_cursor_credit_grants(&serde_json::json!({"grantTotal": 150, "grants": []})),
        150
    );
    let credits = cursor_credits_bucket(1500, 250);
    assert_eq!(credits.label, "Credits");
    assert_eq!(credits.used_label.as_deref(), Some("$17.50"));
    assert_eq!(
        credits.used_money.as_ref().map(|money| money.amount_minor),
        Some(1750)
    );
    assert_eq!(
        parse_cursor_stripe_balance(&serde_json::json!({"balanceCents": 250})),
        250
    );
}

#[test]
fn grok_bot_pooled_or_zero_yields_no_meter() {
    assert!(
        parse_cursor_sand_usage(&serde_json::json!({"usesPooledEnterpriseAllowance": true}))
            .is_none()
    );
    assert!(parse_cursor_sand_usage(&serde_json::json!({"includedLimitZero": true})).is_none());
    assert!(
        parse_cursor_sand_usage(&serde_json::json!({"hasNonZeroIncludedLimit": false})).is_none()
    );
    let sand = parse_cursor_sand_usage(&serde_json::json!({
        "usagePercent": 25,
        "nextResetTimestampUtc": 1_782_000_000,
        "hasNonZeroIncludedLimit": true
    }))
    .expect("meter parses");
    let bucket = cursor_sand_bucket(&sand, 1_781_728_000);
    assert_eq!(bucket.label, "Grok Bot");
    assert_eq!(bucket.remaining_percent, Some(75));
}

#[test]
fn summary_preserves_every_pool_without_invention() {
    let summary = parse_cursor_usage_summary(&serde_json::from_str(SUMMARY_FIXTURE).expect("json"))
        .expect("summary parses");
    assert_eq!(summary.membership.as_deref(), Some("Pro"));
    let buckets = cursor_summary_buckets(&summary, 1_781_728_000);
    let labels: Vec<&str> = buckets.iter().map(|bucket| bucket.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Billing cycle",
            "Auto",
            "API",
            "On-demand (actual)",
            "Overall",
            "Team · On-demand",
            "Team · Pooled"
        ]
    );
    assert_eq!(buckets[0].remaining_percent, Some(59));
    assert!(buckets[0].resets_at.is_some());
    // `overall` has no documented unit: raw, uninterrupted.
    assert_eq!(buckets[4].used_label.as_deref(), Some("77"));
    assert!(buckets[4].used_money.is_none());
}

#[test]
fn requests_bucket_from_model_keyed_shape() {
    let usage = parse_cursor_request_usage(&serde_json::json!({
        "gpt-4": {"maxRequestUsage": 500, "numRequestsTotal": 125},
        "startOfMonth": "2026-09-01T00:00:00Z"
    }))
    .expect("requests parse");
    let bucket = cursor_request_bucket(&usage);
    assert_eq!(bucket.used_label.as_deref(), Some("125"));
    assert_eq!(bucket.limit_label.as_deref(), Some("500"));
    assert_eq!(bucket.remaining_percent, Some(75));
}

#[test]
fn requests_multi_model_pick_is_pinned_not_arbitrary() {
    // Two models with counters: the alphabetically first key wins,
    // deterministically, regardless of document order.
    for value in [
        serde_json::json!({
            "zebra": {"maxRequestUsage": 100, "numRequestsTotal": 90},
            "alpha": {"maxRequestUsage": 500, "numRequestsTotal": 125}
        }),
        serde_json::json!({
            "alpha": {"maxRequestUsage": 500, "numRequestsTotal": 125},
            "zebra": {"maxRequestUsage": 100, "numRequestsTotal": 90}
        }),
    ] {
        let usage = parse_cursor_request_usage(&value).expect("requests parse");
        assert_eq!((usage.used, usage.limit), (125, 500));
    }
}

#[test]
fn enterprise_keeps_actual_and_estimated_apart() {
    let spend = parse_cursor_team_spend(&serde_json::json!({
        "chargedAmount": 120.5,
        "estimatedCost": 99.25,
        "periodEnd": "2026-10-01T00:00:00Z",
        "members": [
            {"email": "a@example.com", "chargedAmount": 70},
            {"email": "b@example.com", "spend": 50.5}
        ]
    }))
    .expect("spend parses");
    let buckets = cursor_team_spend_buckets(&spend, 1_781_728_000);
    assert_eq!(buckets[0].label, "Team spend (actual)");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Spend));
    assert_eq!(buckets[1].label, "Estimated model cost");
    assert_eq!(buckets[1].status_slot, None);
    assert_eq!(
        buckets[1].pace_label.as_deref(),
        Some("estimate · not billed")
    );
    assert_eq!(buckets[2].label, "Team · a@example.com");
    assert_eq!(buckets[3].label, "Team · b@example.com");
    assert!(parse_cursor_team_spend(&serde_json::json!({"unrelated": true})).is_none());
}

#[test]
fn events_aggregate_without_overview_polling() {
    let events = parse_cursor_usage_events(&serde_json::json!({
        "events": [
            {"model": "m", "tokens": 1000, "chargedAmount": 2.5, "estimatedCost": 2.0},
            {"model": "m", "tokens": 500, "chargedAmount": 1.0, "estimatedCost": 1.0}
        ]
    }));
    assert_eq!(events.event_count, 2);
    let buckets = cursor_events_buckets(&events);
    assert_eq!(buckets.len(), 3);
    assert_eq!(buckets[0].label, "Events · Charged (actual)");
    assert!(
        buckets[0]
            .pace_label
            .as_deref()
            .is_some_and(|pace| pace.contains("hourly aggregated"))
    );
    assert_eq!(buckets[2].used_label.as_deref(), Some("1.5K"));
}

#[test]
fn auth_parses_either_token_key_and_rejects_blank() {
    let auth = cursor_auth_from_value(&serde_json::json!({"accessToken": "  tok  "}))
        .expect("camelCase key parses");
    assert_eq!(auth.access_token, "tok");
    assert_eq!(auth.user_id, None);
    assert!(
        cursor_auth_from_value(&serde_json::json!({"access_token": "tok"}))
            .is_some_and(|auth| auth.access_token == "tok")
    );
    assert!(cursor_auth_from_value(&serde_json::json!({"accessToken": "  "})).is_none());
    assert!(cursor_auth_from_value(&serde_json::json!({"other": 1})).is_none());
}

#[test]
fn cli_identity_reads_auth_info_display_keys() {
    assert_eq!(
        cursor_cli_identity_from_value(
            &serde_json::json!({"authInfo": {"email": "a@example.test"}})
        )
        .as_deref(),
        Some("a@example.test")
    );
    assert_eq!(
        cursor_cli_identity_from_value(&serde_json::json!({"authInfo": {"displayName": "Ada"}}))
            .as_deref(),
        Some("Ada")
    );
    assert!(cursor_cli_identity_from_value(&serde_json::json!({"authInfo": {}})).is_none());
    assert!(cursor_cli_identity_from_value(&serde_json::json!({})).is_none());
}

fn canned_dashboard_response(request: &[u8]) -> &'static str {
    let text = String::from_utf8_lossy(request);
    if text.contains("GetCurrentPeriodUsage") {
        PERIOD_FIXTURE
    } else if text.contains("GetPlanInfo") {
        r#"{"planName": "pro_plus"}"#
    } else if text.contains("GetCreditGrantsBalance") {
        r#"{"grantTotal": 1500}"#
    } else {
        "{}"
    }
}

#[test]
fn snapshot_with_auth_serves_personal_quota_from_canned_rpc() {
    use std::io::{Read as _, Write as _};

    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    // Four personal-RPC calls; the opaque fixture token yields no user id so
    // session-REST enrichment stays skipped.
    let server = std::thread::spawn(move || {
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).unwrap();
            let body = canned_dashboard_response(&request[..read]);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let auth = cursor_auth_from_value(&serde_json::json!({"accessToken": "fixture-opaque"}))
        .expect("fixture auth parses");
    let view = cursor_snapshot_with_auth(
        "cursor",
        Some("Cursor"),
        &auth,
        Some("cursor@example.test"),
        "OAuth · configured profile",
        &format!("http://{address}"),
        1_781_728_000,
    );
    server.join().unwrap();

    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.account.provider_label, "Cursor");
    assert_eq!(view.account.account_label, "cursor@example.test");
    assert_eq!(
        view.account.credential_origin.as_deref(),
        Some("OAuth · configured profile")
    );
    assert_eq!(view.account.plan_label.as_deref(), Some("Pro Plus"));
    assert!(
        view.buckets
            .iter()
            .any(|bucket| bucket.label == "Billing cycle")
    );
    assert!(view.last_error.is_none());
}

#[test]
fn snapshot_with_auth_refused_base_is_stale_never_fabricated() {
    let refused = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = refused.local_addr().unwrap();
    drop(refused);
    let auth = cursor_auth_from_value(&serde_json::json!({"accessToken": "fixture-opaque"}))
        .expect("fixture auth parses");
    let view = cursor_snapshot_with_auth(
        "cursor",
        Some("Cursor"),
        &auth,
        None,
        "OAuth · configured profile",
        &format!("http://{address}"),
        1_781_728_000,
    );

    assert_eq!(view.status, UsageSnapshotStatus::Stale);
    assert_eq!(view.account.provider_label, "Cursor");
    assert!(view.last_error.is_some());
}
