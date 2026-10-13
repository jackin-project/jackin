// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn kimi_code_api_pools_map_rolling_weekly_monthly() {
    let usage: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usage": {
            "limit": 1000,
            "used": 220,
            "reset_time": "2026-06-18T12:00:00Z",
            "title": "Weekly summary"
        },
        "limits": [{
            "window": { "duration": 5, "timeUnit": "TIME_UNIT_HOUR" },
            "detail": { "limit": 200, "remaining": 150 }
        }],
        "usages": {
            "limit_5h": {
                "limit": 200,
                "used": 50,
                "reset_time": "2026-06-11T16:00:00Z"
            },
            "limit_7d": { "used_ratio": 0.22, "reset_at": 1_782_000_000_i64 },
            "limit_month_total": {
                "limit": "5000",
                "used": "1000",
                "resetTime": "2026-07-01T00:00:00Z"
            }
        },
        "user": {
            "id": "user-123",
            "email": "dev@example.test",
            "membership": { "level": "LEVEL_INTERMEDIATE" }
        },
        "version": "GOODS_VERSION_V1"
    }))
    .expect("valid Kimi Code API usage");

    let buckets = usage.buckets(1_781_185_560);
    // Pools supersede the coarser summary/limits shapes: no window twice.
    assert_eq!(buckets.len(), 3);
    assert_eq!(buckets[0].label, "5-hour");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[0].used_label.as_deref(), Some("50"));
    assert_eq!(buckets[0].limit_label.as_deref(), Some("200"));
    assert_eq!(buckets[0].remaining_percent, Some(75));
    assert_eq!(buckets[1].label, "Weekly");
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[1].remaining_percent, Some(78));
    assert_eq!(buckets[1].resets_at, Some(1_782_000_000));
    assert_eq!(buckets[2].label, "Monthly");
    assert_eq!(buckets[2].status_slot, None);
    assert_eq!(buckets[2].remaining_percent, Some(80));

    let (account, username, plan) = kimi_account_identity(&usage);
    assert_eq!(account, "dev@example.test");
    assert_eq!(username.as_deref(), Some("dev@example.test"));
    assert_eq!(plan.as_deref(), Some("Allegretto"));
}

#[test]
fn kimi_membership_plan_mapping_is_version_gated() {
    assert_eq!(
        kimi_membership_plan("LEVEL_FREE", None),
        "Adagio".to_owned()
    );
    assert_eq!(
        kimi_membership_plan("LEVEL_ADVANCED", Some("GOODS_VERSION_V1")),
        "Allegro".to_owned()
    );
    // Unknown goods version: raw level through, never a stale mapping.
    assert_eq!(
        kimi_membership_plan("LEVEL_BASIC", Some("GOODS_VERSION_V2")),
        "Level Basic".to_owned()
    );
    // Unknown level: prefix stripped, humanized.
    assert_eq!(
        kimi_membership_plan("LEVEL_FOUNDER", None),
        "Founder".to_owned()
    );
}

#[test]
fn kimi_identity_falls_back_through_name_to_id() {
    let usage: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "user": { "id": "user-9", "name": "Dev Nine" }
    }))
    .expect("identity-only usage");
    let (account, username, plan) = kimi_account_identity(&usage);
    assert_eq!(account, "Dev Nine");
    assert_eq!(username.as_deref(), Some("Dev Nine"));
    assert_eq!(plan, None);

    let anonymous: KimiUsageResponse =
        serde_json::from_value(serde_json::json!({})).expect("empty usage");
    assert_eq!(
        kimi_account_identity(&anonymous),
        (String::new(), None, None)
    );
}

#[test]
fn kimi_numeric_detail_and_snake_reset_parse() {
    let usage: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usage": {
            "limit": 1000,
            "remaining": 780,
            "reset_time": "2026-06-18T12:00:00Z"
        }
    }))
    .expect("numeric detail usage");
    let buckets = usage.buckets(1_781_185_560);
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].label, "Weekly");
    assert_eq!(buckets[0].used_label.as_deref(), Some("220"));
    assert_eq!(buckets[0].remaining_percent, Some(78));
    assert!(buckets[0].resets_at.is_some());

    // Epoch-millisecond reset normalizes to seconds.
    let ms: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usage": { "limit": 100, "used": 10, "reset_at": 1_782_000_000_000_i64 }
    }))
    .expect("ms reset usage");
    let buckets = ms.buckets(1_781_185_560);
    assert_eq!(buckets[0].resets_at, Some(1_782_000_000));
}

#[test]
fn kimi_extra_usage_wallet_maps_to_spend_bucket() {
    let usage: KimiLocalUsage = serde_json::from_value(serde_json::json!({
        "extra_usage": {
            "monthly_cap_cents": 2000,
            "monthly_used_cents": 500,
            "currency": "USD"
        }
    }))
    .expect("wallet usage");
    let wallet = usage.wallet().expect("no in-band error").expect("wallet");
    let view = kimi_extra_usage_bucket(wallet).expect("spend bucket");
    assert_eq!(view.label, "Extra usage");
    assert_eq!(view.status_slot, Some(StatusSlot::Spend));
    assert_eq!(view.remaining_percent, Some(75));
    assert_eq!(
        view.used_money.as_ref().map(|money| money.amount_minor),
        Some(500)
    );
    assert_eq!(
        view.used_money
            .as_ref()
            .map(|money| money.currency.as_str()),
        Some("USD")
    );
    assert_eq!(
        view.limit_money.as_ref().map(|money| money.amount_minor),
        Some(2000)
    );

    // Balance/total pair without a monthly cap: consumed of total.
    let pair: KimiLocalUsage = serde_json::from_value(serde_json::json!({
        "extra_usage": { "balance": 7500, "total": 10000 }
    }))
    .expect("balance-pair usage");
    let wallet = pair.wallet().expect("ok").expect("wallet");
    let view = kimi_extra_usage_bucket(wallet).expect("spend bucket");
    assert_eq!(view.remaining_percent, Some(75));
    assert_eq!(
        view.used_money
            .as_ref()
            .map(|money| money.currency.as_str()),
        Some("credits")
    );

    // HTTP-200 in-band error rejects the body.
    let failed: KimiLocalUsage = serde_json::from_value(serde_json::json!({
        "error": { "code": "UNAUTHENTICATED", "message": "bad credential" }
    }))
    .expect("error usage");
    assert_eq!(
        failed.wallet(),
        Err("Kimi local usage error: bad credential".to_owned())
    );
}

#[test]
fn kimi_unitless_wallet_keys_are_unknown_scale_never_money() {
    // Unitless aliases no longer feed `Money`: they render as a label-only
    // row so a major-unit value can never go 100x off in the headline.
    let usage: KimiLocalUsage = serde_json::from_value(serde_json::json!({
        "extra_usage": { "monthly_used": 500, "monthly_cap": 2000 }
    }))
    .expect("unitless wallet");
    let wallet = usage.wallet().expect("ok").expect("wallet");
    assert_eq!(wallet.monthly_used, None);
    assert_eq!(wallet.monthly_cap, None);
    let view = kimi_extra_usage_bucket(wallet).expect("label-only bucket");
    assert_eq!(view.label, "Extra usage");
    assert_eq!(view.used_label.as_deref(), Some("2000"));
    assert_eq!(
        view.pace_label.as_deref(),
        Some("monthly_cap · unknown scale")
    );
    assert!(view.used_money.is_none());
    assert!(view.limit_money.is_none());
    assert_eq!(view.status_slot, None);
    assert_eq!(view.remaining_percent, None);

    // Evidenced `*_cents` keys still feed `Money` when present alongside.
    let mixed: KimiLocalUsage = serde_json::from_value(serde_json::json!({
        "extra_usage": {
            "monthly_used_cents": 500,
            "monthly_cap_cents": 2000,
            "used": 999
        }
    }))
    .expect("mixed wallet");
    let wallet = mixed.wallet().expect("ok").expect("wallet");
    let view = kimi_extra_usage_bucket(wallet).expect("spend bucket");
    assert_eq!(view.status_slot, Some(StatusSlot::Spend));
    assert_eq!(
        view.used_money.as_ref().map(|money| money.amount_minor),
        Some(500)
    );
}

#[test]
fn kimi_over_cap_keeps_raw_percent_with_clamped_bar() {
    let usage: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usages": {
            "limit_5h": { "limit": 100, "used": 140 },
            "limit_7d": { "used_ratio": 150.0 }
        }
    }))
    .expect("over-cap pools");
    let buckets = usage.buckets(1_781_185_560);
    assert_eq!(buckets[0].remaining_percent, Some(0));
    assert_eq!(
        buckets[0].pace_label.as_deref(),
        Some("140% used"),
        "raw overage stays visible"
    );
    assert_eq!(buckets[0].used_label.as_deref(), Some("140"));
    // `used_ratio` above 1.0 is an already-scaled percent: 150 → 150%.
    assert_eq!(buckets[1].remaining_percent, Some(0));
    assert_eq!(buckets[1].pace_label.as_deref(), Some("150% used"));

    // At/below cap: no over-cap prefix, pace untouched.
    assert_eq!(kimi_over_cap_label(100.0), None);
    assert_eq!(kimi_over_cap_label(42.5), None);
    assert_eq!(kimi_over_cap_label(f64::NAN), None);
}

#[test]
fn kimi_usages_url_normalizes_coding_base_variants() {
    assert_eq!(
        kimi_usages_url_from_base(None),
        "https://api.kimi.com/coding/v1/usages"
    );
    assert_eq!(
        kimi_usages_url_from_base(Some("https://api.kimi.com/coding/v1")),
        "https://api.kimi.com/coding/v1/usages"
    );
    assert_eq!(
        kimi_usages_url_from_base(Some("https://api.kimi.com/coding/")),
        "https://api.kimi.com/coding/v1/usages"
    );
    assert_eq!(
        kimi_usages_url_from_base(Some("api.kimi.com")),
        "https://api.kimi.com/coding/v1/usages"
    );
    assert_eq!(
        kimi_usages_url_from_base(Some("https://proxy.test/kimi/coding/v1/usages")),
        "https://proxy.test/kimi/coding/v1/usages"
    );
}
