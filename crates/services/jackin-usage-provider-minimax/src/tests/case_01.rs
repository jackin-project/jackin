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
        Some(108.0)
    );
    assert_eq!(minimax_effective_remaining(Some(90.0), None), Some(90.0));
    assert_eq!(
        minimax_effective_remaining(Some(90.0), Some(1000.0)),
        Some(90.0)
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
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(
        buckets[0].provider_quota_state,
        Some(jackin_protocol::control::ProviderQuotaState::Exhausted)
    );
    assert_eq!(
        buckets[0]
            .count_quota
            .map(|count| (count.used, count.limit, count.remaining, count.unit)),
        Some((
            Some(100),
            Some(100),
            None,
            jackin_protocol::control::CountQuotaUnit::Unknown
        ))
    );
    assert_eq!(buckets[0].pace_label.as_deref(), Some("Usage: 100 / 100"));
    assert_eq!(buckets[1].label, "General · Weekly");
    assert_eq!(
        buckets[1].provider_quota_state,
        Some(jackin_protocol::control::ProviderQuotaState::Unlimited)
    );
    assert_eq!(
        buckets[1]
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(99.0)
    );
    assert_eq!(
        buckets[1].remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(buckets[1].pace_label.as_deref(), Some("Usage: 10 / 700"));
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
}

#[test]
fn minimax_fractional_remaining_stays_source_evidence_when_boosted() {
    let usage: MiniMaxUsageResponse = serde_json::from_value(serde_json::json!({
        "base_resp": { "status_code": 0 },
        "model_remains": [{
            "model_name": "general",
            "current_interval_remaining_percent": 33.375,
            "current_interval_boost_permille": 1200,
            "current_interval_status": 0
        }]
    }))
    .expect("valid MiniMax fractional usage");

    let bucket = usage.buckets(1_782_315_600).remove(0);
    assert_eq!(
        bucket
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(33.375)
    );
    assert_eq!(
        bucket.remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
    );
}

#[test]
fn minimax_explicit_states_are_usable_without_inventing_quantities() {
    let usage: MiniMaxUsageResponse = serde_json::from_value(serde_json::json!({
        "model_remains": [{
            "model_name": "general",
            "current_interval_status": 2,
            "current_weekly_status": 3
        }]
    }))
    .expect("state-only MiniMax response");
    let buckets = usage.buckets(1_782_315_600);
    assert_eq!(buckets.len(), 2);
    assert!(
        buckets
            .iter()
            .all(|bucket| bucket.status == UsageSnapshotStatus::Fresh)
    );
    assert!(buckets.iter().all(|bucket| bucket.count_quota.is_none()));
    assert!(buckets.iter().all(|bucket| bucket.used_percent.is_none()));
    assert!(
        buckets
            .iter()
            .all(|bucket| bucket.remaining_percent.is_none())
    );
    assert_eq!(
        buckets[0].provider_quota_state,
        Some(jackin_protocol::control::ProviderQuotaState::Exhausted)
    );
    assert_eq!(
        buckets[1].provider_quota_state,
        Some(jackin_protocol::control::ProviderQuotaState::Unlimited)
    );
    assert!(buckets.iter().all(|bucket| bucket.pace_label.is_none()));
}

#[test]
fn minimax_explicit_state_preserves_conflicting_count_and_percent_observations() {
    let usage: MiniMaxUsageResponse = serde_json::from_value(serde_json::json!({
        "model_remains": [{
            "model_name": "general",
            "current_interval_total_count": 10,
            "current_interval_usage_count": 1,
            "current_interval_remaining_percent": 57.125,
            "current_interval_status": 2,
            "current_weekly_total_count": "10",
            "current_weekly_usage_count": "0",
            "current_weekly_remaining_percent": 0,
            "current_weekly_status": 3
        }]
    }))
    .expect("contradictory source observations remain decodable");
    let buckets = usage.buckets(1_782_315_600);

    assert_eq!(
        buckets[0]
            .count_quota
            .map(|count| (count.used, count.limit, count.remaining)),
        Some((Some(1), Some(10), None))
    );
    assert_eq!(
        buckets[0]
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(57.125)
    );
    assert_eq!(
        buckets[0].remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        buckets[0].provider_quota_state,
        Some(jackin_protocol::control::ProviderQuotaState::Exhausted)
    );
    assert_eq!(
        buckets[1]
            .count_quota
            .map(|count| (count.used, count.limit)),
        Some((Some(0), Some(10)))
    );
    assert_eq!(
        buckets[1]
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(0.0)
    );
    assert_eq!(
        buckets[1].provider_quota_state,
        Some(jackin_protocol::control::ProviderQuotaState::Unlimited)
    );
}

#[test]
fn minimax_invalid_fields_are_isolated_and_do_not_create_fresh_empty_buckets() {
    let usage: MiniMaxUsageResponse = serde_json::from_value(serde_json::json!({
        "model_remains": [
            {
                "model_name": "general",
                "current_interval_total_count": "18446744073709551615",
                "current_interval_usage_count": -1,
                "current_interval_remaining_percent": "bad",
                "current_interval_status": "2"
            },
            {
                "model_name": "other",
                "current_interval_total_count": "18446744073709551616",
                "current_interval_usage_count": 1.5,
                "current_interval_remaining_percent": -1,
                "current_interval_status": "unsupported"
            }
        ]
    }))
    .expect("invalid fields do not reject sibling models");
    let buckets = usage.buckets(1_782_315_600);
    let general = buckets
        .iter()
        .find(|bucket| bucket.label == "General · 5h")
        .expect("valid count sibling remains");
    assert_eq!(general.status, UsageSnapshotStatus::Fresh);
    assert_eq!(
        general.count_quota.map(|count| (count.used, count.limit)),
        Some((None, Some(u64::MAX)))
    );
    assert_eq!(general.provider_quota_state, None);
    assert!(
        general
            .pace_label
            .as_deref()
            .is_some_and(|note| note.contains("invalid provider fields"))
    );

    let other = buckets
        .iter()
        .find(|bucket| bucket.label == "Other")
        .expect("invalid row retained diagnostically");
    assert_eq!(other.status, UsageSnapshotStatus::Unavailable);
    assert!(other.count_quota.is_none());
    assert!(
        other
            .pace_label
            .as_deref()
            .is_some_and(|note| note.contains("invalid provider fields"))
    );
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
    assert_eq!(minimax_duration_seconds(14_400_000), 14_400);
    assert_eq!(minimax_duration_seconds(345_600_000), 345_600);
    assert_eq!(minimax_duration_seconds(3_600), 3_600);
    assert_eq!(
        minimax_reset_epoch(None, Some(14_400_000), 1_782_315_600),
        Some(1_782_330_000)
    );
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
