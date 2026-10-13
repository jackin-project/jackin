// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn minimax_key_product_selects_balance_route() {
    assert_eq!(
        minimax_key_product("sk-api-secret"),
        MiniMaxKeyProduct::Payg
    );
    assert_eq!(
        minimax_key_product("  sk-api-secret"),
        MiniMaxKeyProduct::Payg
    );
    assert_eq!(
        minimax_key_product("token-plan-key"),
        MiniMaxKeyProduct::TokenPlan
    );
}

#[test]
fn minimax_fetch_plan_pins_region_and_product() {
    let global = minimax_fetch_plan_from(MiniMaxKeyProduct::TokenPlan, None, None, None);
    assert_eq!(global.region, MiniMaxRegion::Global);
    assert_eq!(global.host_label, "api.minimax.io");
    assert_eq!(
        global.urls,
        vec![
            "https://api.minimax.io/v1/token_plan/remains",
            "https://api.minimax.io/v1/api/openplatform/coding_plan/remains",
            "https://www.minimax.io/v1/token_plan/remains",
        ]
    );

    let china = minimax_fetch_plan_from(MiniMaxKeyProduct::TokenPlan, None, None, Some("cn"));
    assert_eq!(china.region, MiniMaxRegion::China);
    assert_eq!(
        china.urls,
        vec![
            "https://api.minimaxi.com/v1/token_plan/remains",
            "https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains",
        ]
    );

    let payg = minimax_fetch_plan_from(MiniMaxKeyProduct::Payg, None, None, None);
    assert_eq!(
        payg.urls,
        vec!["https://api.minimax.io/account/query_balance"]
    );

    let cn_host = minimax_fetch_plan_from(
        MiniMaxKeyProduct::Payg,
        None,
        Some("https://api.minimaxi.com/v1"),
        None,
    );
    assert_eq!(cn_host.region, MiniMaxRegion::China);
    assert_eq!(
        cn_host.urls,
        vec!["https://api.minimaxi.com/account/query_balance"]
    );
    assert_eq!(cn_host.host_label, "api.minimaxi.com");
}

#[test]
fn minimax_boost_scales_remaining_past_cap() {
    assert_eq!(
        minimax_effective_remaining(Some(90.0), Some(1200.0)),
        Some(108)
    );
    assert_eq!(minimax_effective_remaining(Some(90.0), None), Some(90));
    assert_eq!(
        minimax_effective_remaining(Some(90.0), Some(1000.0)),
        Some(90)
    );
    assert_eq!(minimax_effective_remaining(None, Some(1200.0)), None);
    assert_eq!(
        minimax_boost_note(Some(1200.0)).as_deref(),
        Some("+20% boost")
    );
    assert_eq!(minimax_boost_note(Some(1000.0)), None);
    assert_eq!(minimax_boost_note(None), None);
}

#[test]
fn minimax_exhausted_and_unlimited_windows_render() {
    let usage: MiniMaxUsageResponse = serde_json::from_value(serde_json::json!({
        "base_resp": { "status_code": 0 },
        "model_remains": [
            {
                "model_name": "general",
                "current_interval_total_count": 100,
                "current_interval_usage_count": 100,
                "current_interval_status": 2,
                "current_weekly_total_count": 700,
                "current_weekly_usage_count": 10,
                "current_weekly_remaining_percent": 99,
                "current_weekly_status": 3
            }
        ]
    }))
    .expect("valid MiniMax usage");
    let buckets = usage.buckets(1_782_315_600);
    assert_eq!(buckets.len(), 2);
    assert_eq!(buckets[0].label, "General · 5h");
    assert_eq!(buckets[0].remaining_percent, Some(0));
    assert_eq!(
        buckets[0].pace_label.as_deref(),
        Some("Usage: 100 / 100 · Exhausted")
    );
    assert_eq!(buckets[1].label, "General · Weekly");
    assert_eq!(buckets[1].remaining_percent, None);
    assert_eq!(buckets[1].pace_label.as_deref(), Some("Unlimited"));
    // Slots still tag the general model's windows.
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
}

#[test]
fn minimax_balance_maps_amounts_with_currency() {
    let balance: MiniMaxBalanceResponse = serde_json::from_value(serde_json::json!({
        "base_resp": { "status_code": 0 },
        "available_amount": "12.50",
        "cash_balance": "10.00",
        "voucher_balance": "2.50",
        "credit_balance": "0.00",
        "owed_amount": "0.00"
    }))
    .expect("valid MiniMax balance");
    balance.validate().expect("valid balance");
    let buckets = balance.buckets(MiniMaxRegion::Global);
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].label, "Balance");
    assert_eq!(buckets[0].used_label.as_deref(), Some("12.50"));
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(
        buckets[0].pace_label.as_deref(),
        Some("USD · cash 10.00 · voucher 2.50 · credit 0.00 · owed 0.00")
    );

    let cn = balance.buckets(MiniMaxRegion::China);
    assert!(
        cn[0]
            .pace_label
            .as_deref()
            .unwrap_or_default()
            .starts_with("CNY · ")
    );

    let empty: MiniMaxBalanceResponse =
        serde_json::from_value(serde_json::json!({"base_resp": {"status_code": 0}}))
            .expect("empty balance");
    assert_eq!(
        empty.validate(),
        Err("missing MiniMax balance data".to_owned())
    );
}

#[test]
fn minimax_decimal_minor_parses_without_float_rounding() {
    assert_eq!(minimax_decimal_minor("12.50"), Some(1250));
    assert_eq!(minimax_decimal_minor("12.5"), Some(1250));
    assert_eq!(minimax_decimal_minor("12"), Some(1200));
    assert_eq!(minimax_decimal_minor("-3.456"), Some(-345));
    assert_eq!(minimax_decimal_minor("0.00"), Some(0));
    assert_eq!(minimax_decimal_minor(""), None);
    assert_eq!(minimax_decimal_minor("12.5x"), None);
    assert_eq!(minimax_decimal_minor("abc"), None);
}

#[test]
fn minimax_remains_time_normalizes_milliseconds() {
    // Live millisecond durations collapse to seconds; small values pass
    // through as seconds.
    assert_eq!(minimax_duration_seconds(14_400_000), 14_400);
    assert_eq!(minimax_duration_seconds(345_600_000), 345_600);
    assert_eq!(minimax_duration_seconds(3_600), 3_600);
    assert_eq!(
        minimax_reset_epoch(None, Some(14_400_000), 1_782_315_600),
        Some(1_782_330_000)
    );
    // Explicit end epochs win over durations.
    assert_eq!(
        minimax_reset_epoch(Some(1_782_400_000), Some(14_400_000), 1_782_315_600),
        Some(1_782_400_000)
    );
}

#[test]
fn minimax_operation_path_covers_balance_endpoint() {
    assert_eq!(
        minimax_operation_path("https://api.minimax.io/account/query_balance"),
        "/account/query_balance"
    );
}
