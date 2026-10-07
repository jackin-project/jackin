// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn summary_bucket_selects_first_ranked_limit_over_tighter_spend() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": { "utilization": 11.0, "resets_at": "2026-06-28T16:40:00Z" },
        "seven_day": { "utilization": 27.0, "resets_at": "2026-07-03T07:00:00Z" },
        "spend": {
            "used": { "amount_minor": 9000, "currency": "USD", "exponent": 2 },
            "limit": { "amount_minor": 30000, "currency": "USD", "exponent": 2 },
            "percent": 30,
            "enabled": true
        }
    }))
    .expect("valid Claude OAuth usage");
    // Spend = 70% left (tightest), Session = 89%, Weekly = 73%: the Weekly
    // long-range window wins by rank, not by tightness.
    let buckets = usage.into_buckets(1_781_185_560);
    let chosen = summary_bucket(&buckets).expect("a ranked bucket");
    assert_eq!(chosen.status_slot, Some(StatusSlot::Weekly));
    assert_eq!(chosen.remaining_percent, Some(73));
}

#[test]
fn usage_tab_status_label_selects_ranked_limit_and_names_unslotted_winner() {
    let reset_at = "2026-07-03T06:59:59Z";
    // Fable (10% left) is tighter than Session (50% left) and Weekly (60%),
    // but Weekly wins by rank and stays bare.
    let ranked_wins: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "limits": [
            { "kind": "session", "group": "session", "percent": 50,
              "severity": "normal", "resets_at": "2026-07-03T03:20:00Z", "scope": null },
            { "kind": "weekly_all", "group": "weekly", "percent": 40,
              "severity": "normal", "resets_at": reset_at, "scope": null },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 90,
              "severity": "danger", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Fable" } } }
        ]
    }))
    .expect("limits response");
    let mut view = FocusedUsageView::unavailable("x", 1_781_300_000);
    view.status = UsageSnapshotStatus::Fresh;
    view.buckets = ranked_wins.into_buckets(1_781_300_000);
    let label = usage_tab_status_label(&view);
    assert!(
        label.starts_with("60% left"),
        "ranked weekly must win over tighter fable: got {label:?}"
    );

    // Nothing slotted carries a percent: the unslotted Fable window wins and
    // is named.
    let fable_wins: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "limits": [
            { "kind": "weekly_scoped", "group": "weekly", "percent": 90,
              "severity": "danger", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Fable" } } }
        ]
    }))
    .expect("limits response");
    view.buckets = fable_wins.into_buckets(1_781_300_000);
    let label = usage_tab_status_label(&view);
    assert!(
        label.starts_with("Fable 10% left"),
        "unslotted winner must be named: got {label:?}"
    );
}

#[test]
fn codex_oauth_response_maps_primary_weekly_spark_and_credits() {
    let mut usage: CodexUsageResponse = serde_json::from_value(serde_json::json!({
        "plan_type": "pro",
        "rate_limit": {
            "primary_window": {
                "used_percent": 63,
                "reset_at": 1781189520,
                "limit_window_seconds": 18000
            },
            "secondary_window": {
                "used_percent": 90,
                "reset_at": 1781197200,
                "limit_window_seconds": 604800
            }
        },
        "additional_rate_limits": [{
            "limit_name": "gpt-5.3-codex-spark",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 0,
                    "reset_at": 1781200800,
                    "limit_window_seconds": 18000
                },
                "secondary_window": {
                    "used_percent": 0,
                    "reset_at": 1781798400,
                    "limit_window_seconds": 604800
                }
            }
        }],
        "credits": {
            "has_credits": true,
            "unlimited": false,
            "balance": "12.5"
        }
    }))
    .expect("valid Codex usage");
    usage.reset_credits = Some(CodexResetCredits {
        available_count: 2,
        credits: vec![
            CodexResetCredit {
                status: Some("available".to_owned()),
                expires_at: Some("2026-06-10T00:00:00Z".to_owned()),
            },
            CodexResetCredit {
                status: Some("available".to_owned()),
                expires_at: Some("2026-06-18T00:00:00Z".to_owned()),
            },
            CodexResetCredit {
                status: Some("redeemed".to_owned()),
                expires_at: Some("2026-06-17T00:00:00Z".to_owned()),
            },
        ],
    });

    let buckets = usage.buckets(1_781_185_560);

    assert_eq!(buckets[0].label, "Session");
    assert_eq!(buckets[0].remaining_percent, Some(37));
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[1].label, "Weekly");
    assert_eq!(buckets[1].remaining_percent, Some(10));
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert!(buckets.iter().any(
        |bucket| bucket.label == "Codex Spark 5-hour" && bucket.remaining_percent == Some(100)
    ));
    // The per-feature Spark detail rows are not headline slots.
    assert!(buckets.iter().all(|bucket| {
        !bucket.label.starts_with("Codex Spark") || bucket.status_slot.is_none()
    }));
    let reset_credits = buckets
        .iter()
        .position(|bucket| bucket.label == "Limit Reset Credits")
        .expect("reset credits bucket");
    let reset_credit_label = format!(
        "2 manual resets available · Next expires {}",
        expiry_label(
            parse_iso_epoch("2026-06-18T00:00:00Z").expect("expiry epoch"),
            1_781_185_560
        )
    );
    assert_eq!(
        buckets[reset_credits].pace_label.as_deref(),
        Some(reset_credit_label.as_str())
    );
    let credits = buckets
        .iter()
        .enumerate()
        .find(|(_, bucket)| bucket.label == "Credits")
        .expect("credits bucket");
    assert!(reset_credits < credits.0);
    assert_eq!(credits.1.limit_label.as_deref(), Some("12.50 credits"));
}

#[test]
fn codex_rpc_response_maps_account_windows_and_credits() {
    let limits: CodexRpcRateLimitsResponse = serde_json::from_value(serde_json::json!({
        "rateLimits": {
            "primary": {
                "usedPercent": 63.0,
                "windowDurationMins": 300,
                "resetsAt": 1781189520
            },
            "secondary": {
                "usedPercent": 90.0,
                "windowDurationMins": 10080,
                "resetsAt": 1781798400
            },
            "credits": {
                "hasCredits": true,
                "unlimited": false,
                "balance": "12.5"
            },
            "planType": "pro"
        }
    }))
    .expect("valid Codex RPC rate limits");
    let account: CodexRpcAccountResponse = serde_json::from_value(serde_json::json!({
        "account": {
            "type": "chatgpt",
            "email": "person@example.com",
            "planType": "pro"
        }
    }))
    .expect("valid Codex RPC account");

    let usage = CodexRpcUsage::from_rpc(limits, Some(account));
    let buckets = usage.response.buckets(1_781_185_560);

    assert_eq!(usage.account_label.as_deref(), Some("person@example.com"));
    assert_eq!(usage.response.plan_type.as_deref(), Some("pro"));
    assert_eq!(buckets[0].label, "Session");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[0].remaining_percent, Some(37));
    assert_eq!(buckets[0].pace_label.as_deref(), Some("15% in reserve"));
    assert_eq!(buckets[1].label, "Weekly");
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[1].remaining_percent, Some(10));
    assert_eq!(buckets[1].pace_label.as_deref(), Some("1 week window"));
    let credits = buckets
        .iter()
        .find(|bucket| bucket.label == "Credits")
        .expect("credits bucket");
    assert_eq!(credits.limit_label.as_deref(), Some("12.50 credits"));
}

#[test]
fn codex_rpc_account_api_key_tag_yields_origin_label_and_rate_limits() {
    let usage = decode_codex_rpc_usage(
        codex_minimal_limits_value(),
        Some(serde_json::json!({ "account": { "type": "apiKey" } })),
    )
    .expect("Codex RPC decode");
    assert_eq!(usage.account_label.as_deref(), Some("Codex API key"));
    let buckets = usage.response.buckets(1_781_185_560);
    assert!(buckets.iter().any(|bucket| bucket.label == "Session"));
}

#[test]
fn codex_rpc_account_amazon_bedrock_tag_decodes_without_label() {
    let account: CodexRpcAccountResponse = serde_json::from_value(serde_json::json!({
        "account": { "type": "amazonBedrock", "usesCodexManagedCredentials": true }
    }))
    .expect("Codex Bedrock account decodes");
    let limits: CodexRpcRateLimitsResponse =
        serde_json::from_value(codex_minimal_limits_value()).expect("limits");
    let usage = CodexRpcUsage::from_rpc(limits, Some(account));
    assert_eq!(usage.account_label, None);
}

#[test]
fn codex_rpc_account_decode_failure_degrades_to_no_label() {
    let usage = decode_codex_rpc_usage(
        codex_minimal_limits_value(),
        Some(serde_json::json!({ "account": { "type": "someFutureTag" } })),
    )
    .expect("unknown account tag still yields usage");
    assert_eq!(usage.account_label, None);
    let buckets = usage.response.buckets(1_781_185_560);
    assert!(buckets.iter().any(|bucket| bucket.label == "Session"));
}
