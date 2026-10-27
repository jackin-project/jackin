// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn openrouter_key_maps_cap_remaining_and_period_spend() {
    let quota = parse_openrouter_key_usage(key_fixture(), 1_780_000_000).expect("key fixture");
    assert_eq!(quota.plan_label.as_deref(), Some("Pay as you go"));
    let cap = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Key Limit")
        .expect("cap row");
    assert_eq!(cap.used_label.as_deref(), Some("$12.50"));
    assert_eq!(cap.limit_label.as_deref(), Some("$100"));
    assert_eq!(
        cap.remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(87.5)
    );
    assert_eq!(
        cap.remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::Derived)
    );
    assert_eq!(
        jackin_usage_provider_core::money_used_percent(cap)
            .map(jackin_protocol::control::RawPercentPoints::get),
        Some(12.5)
    );
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(cap).1,
        Some(jackin_protocol::control::UsageMeterSide::Used)
    );
    assert_eq!(cap.status_slot, Some(StatusSlot::Spend));
    let month = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Spent this month")
        .expect("monthly row");
    assert_eq!(month.used_label.as_deref(), Some("$12.50"));
    assert_eq!(month.remaining_percent, None);
    assert_eq!(
        quota
            .buckets
            .iter()
            .filter(|bucket| bucket.label.starts_with("Spent"))
            .count(),
        3
    );
}

#[test]
fn openrouter_null_cap_means_no_cap_never_infinite() {
    let quota = parse_openrouter_key_usage(
        serde_json::json!({
            "data": {
                "usage_monthly": 12.5,
                "limit": null,
                "limit_remaining": null,
                "is_free_tier": true
            }
        }),
        1_780_000_000,
    )
    .expect("null cap");
    assert_eq!(quota.plan_label.as_deref(), Some("Free tier"));
    assert!(
        quota
            .buckets
            .iter()
            .all(|bucket| bucket.label != "Key Limit"),
        "no cap row without a configured cap"
    );
    assert!(
        quota
            .buckets
            .iter()
            .all(|bucket| bucket.remaining_percent.is_none()),
        "no percentage without a denominator"
    );
    let month = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Spent this month")
        .expect("monthly spend still renders");
    assert_eq!(month.status_slot, Some(StatusSlot::Spend));
}

#[test]
fn openrouter_cap_without_remaining_shows_limit_only() {
    let quota =
        parse_openrouter_key_usage(serde_json::json!({"data": {"limit": 50.0}}), 1_780_000_000)
            .expect("cap without remaining");
    let cap = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Key Limit")
        .expect("cap row");
    assert_eq!(cap.limit_label.as_deref(), Some("$50"));
    assert_eq!(cap.used_label, None);
    assert_eq!(cap.remaining_percent, None);
}

#[test]
fn openrouter_credits_403_is_typed_scope_denial() {
    assert_eq!(
        OpenRouterCreditsOutcome::from_http_status(403),
        OpenRouterCreditsOutcome::ManagementScopeDenied
    );
    assert!(matches!(
        OpenRouterCreditsOutcome::from_http_status(500),
        OpenRouterCreditsOutcome::Unavailable(_)
    ));
}

#[test]
fn openrouter_credits_separates_spend_from_remaining_balance() {
    let outcome = parse_openrouter_credits(serde_json::json!({
        "data": {"total_credits": 100.0, "total_usage": 25.0}
    }))
    .expect("credits fixture");
    let OpenRouterCreditsOutcome::Available {
        spent_cents,
        ceiling_cents,
    } = outcome
    else {
        panic!("expected available credits");
    };
    let view = openrouter_credits_bucket(spent_cents, ceiling_cents);
    assert_eq!(view.used_label.as_deref(), Some("$25"));
    assert_eq!(view.limit_label.as_deref(), Some("$100"));
    assert_eq!(
        view.used_money.as_ref().map(|money| money.amount_minor),
        Some(2_500)
    );
    assert_eq!(
        view.limit_money.as_ref().map(|money| money.amount_minor),
        Some(10_000)
    );
    assert_eq!(
        view.remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(75.0)
    );
    assert_eq!(
        view.remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::Derived)
    );

    let exhausted = openrouter_credits_bucket(1_000, 1_000);
    assert_eq!(exhausted.used_label.as_deref(), Some("$10"));
    assert_eq!(
        exhausted
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(0.0)
    );
    assert_eq!(
        exhausted.remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::Derived)
    );

    let overage = openrouter_credits_bucket(12_000, 10_000);
    assert_eq!(overage.used_label.as_deref(), Some("$120"));
    assert_eq!(overage.remaining_percent, None);
    assert_eq!(
        jackin_usage_provider_core::money_used_percent(&overage)
            .map(jackin_protocol::control::RawPercentPoints::get),
        Some(120.0)
    );
    assert_eq!(
        overage.used_money.as_ref().map(|money| money.amount_minor),
        Some(12_000)
    );

    let bare = openrouter_credits_bucket(0, 0);
    assert_eq!(bare.remaining_percent, None);
}

#[test]
fn openrouter_money_derived_remaining_stays_continuous_and_signed_ratio_stays_raw() {
    use jackin_protocol::control::PercentObservationOrigin::Derived;

    let fractional = openrouter_credits_bucket(1, 3);
    let remaining = fractional
        .remaining_percent
        .map(jackin_protocol::control::CanonicalRemainingPercent::get)
        .expect("derived remaining");
    assert!((remaining - (200.0 / 3.0)).abs() < 1e-12);
    assert_eq!(fractional.remaining_percent_origin, Some(Derived));
    assert_eq!(
        fractional
            .used_money
            .as_ref()
            .map(|money| money.amount_minor),
        Some(1)
    );
    assert_eq!(
        fractional
            .limit_money
            .as_ref()
            .map(|money| money.amount_minor),
        Some(3)
    );
    assert!(
        (jackin_usage_provider_core::money_used_percent(&fractional)
            .expect("money ratio")
            .get()
            - (100.0 / 3.0))
            .abs()
            < 1e-12
    );

    let negative_used = openrouter_credits_bucket(-2_500, 10_000);
    assert_eq!(
        negative_used
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(125.0)
    );
    assert_eq!(negative_used.remaining_percent_origin, Some(Derived));
    assert_eq!(
        jackin_usage_provider_core::money_used_percent(&negative_used)
            .map(jackin_protocol::control::RawPercentPoints::get),
        Some(-25.0)
    );
    assert_eq!(
        negative_used
            .used_money
            .as_ref()
            .map(|money| money.amount_minor),
        Some(-2_500)
    );
}

#[test]
fn openrouter_byok_stays_a_separate_row() {
    let quota = parse_openrouter_key_usage(
        serde_json::json!({
            "data": {
                "usage_monthly": 12.5,
                "limit": 100.0,
                "limit_remaining": 87.5,
                "byok_usage_monthly": 3.0
            }
        }),
        1_780_000_000,
    )
    .expect("byok fixture");
    let byok = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "BYOK spend")
        .expect("byok row");
    assert_eq!(byok.used_label.as_deref(), Some("$3"));
    let cap = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Key Limit")
        .expect("cap row");
    assert_eq!(cap.used_label.as_deref(), Some("$12.50"));
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn openrouter_subcent_cap_preserves_key_limit_and_money_geometry() {
    let quota = parse_openrouter_key_usage(
        serde_json::from_str(
            r#"{"data":{"limit":0.001,"limit_remaining":0.0009,"is_free_tier":false}}"#,
        )
        .expect("subcent key JSON"),
        1_780_000_000,
    )
    .expect("subcent key");
    let cap = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Key Limit")
        .expect("subcent cap row");
    assert_eq!(
        cap.limit_money,
        Some(jackin_protocol::control::Money::new(10, "USD", 4))
    );
    assert_eq!(cap.limit_label.as_deref(), Some("$0.001"));
    assert_eq!(
        cap.used_money,
        Some(jackin_protocol::control::Money::new(1, "USD", 4))
    );
    assert_eq!(cap.used_label.as_deref(), Some("$0.0001"));
    assert_eq!(
        cap.remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(90.0)
    );
    assert_eq!(
        cap.remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::Derived)
    );
    assert_eq!(
        jackin_usage_provider_core::money_used_percent(cap)
            .map(jackin_protocol::control::RawPercentPoints::get),
        Some(10.0)
    );
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn openrouter_period_spend_preserves_decimal_scale() {
    let quota = parse_openrouter_key_usage(
        serde_json::from_str(r#"{"data":{"usage_monthly":0.100000000000000001}}"#)
            .expect("high precision key JSON"),
        1_780_000_000,
    )
    .expect("high precision key");
    let month = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Spent this month")
        .expect("monthly spend row");
    assert_eq!(
        month.used_money,
        Some(jackin_protocol::control::Money::new(
            100_000_000_000_000_001,
            "USD",
            18
        ))
    );
    assert_eq!(month.used_label.as_deref(), Some("$0.100000000000000001"));
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn openrouter_byok_fallback_sums_exact_subcent_values() {
    let quota = parse_openrouter_key_usage(
        serde_json::from_str(r#"{"data":{"byok_spend_a":0.001,"byok_cost_b":0.0009}}"#)
            .expect("subcent BYOK JSON"),
        1_780_000_000,
    )
    .expect("subcent BYOK key");
    let byok = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "BYOK spend")
        .expect("BYOK spend row");
    assert_eq!(
        byok.used_money,
        Some(jackin_protocol::control::Money::new(19, "USD", 4))
    );
    assert_eq!(byok.used_label.as_deref(), Some("$0.0019"));
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn openrouter_byok_sums_fractional_carries_before_narrowing() {
    let quota = parse_openrouter_key_usage(
        serde_json::from_str(
            r#"{"data":{"byok_spend_a":9223372036854775806,"byok_spend_b":0.1,"byok_spend_c":0.9}}"#,
        )
        .expect("fractional carry BYOK JSON"),
        1_780_000_000,
    )
    .expect("fractional carry BYOK key");
    let byok = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "BYOK spend")
        .expect("BYOK spend row");
    assert_eq!(
        byok.used_money,
        Some(jackin_protocol::control::Money::new(i64::MAX, "USD", 0))
    );
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn openrouter_byok_omits_unrepresentable_numeric_fallback_groups() {
    for json in [
        r#"{"data":{"byok_spend_a":1,"byok_cost_b":1e-256}}"#,
        r#"{"data":{"byok_spend_a":1,"byok_cost_b":1e30}}"#,
    ] {
        let quota = parse_openrouter_key_usage(
            serde_json::from_str(json).expect("unrepresentable BYOK JSON"),
            1_780_000_000,
        )
        .expect("unrepresentable BYOK key remains a valid response");
        assert!(
            quota
                .buckets
                .iter()
                .all(|bucket| bucket.label != "BYOK spend")
        );
    }
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn openrouter_byok_omits_final_fallback_overflow() {
    let quota = parse_openrouter_key_usage(
        serde_json::from_str(r#"{"data":{"byok_spend_a":9223372036854775807,"byok_cost_b":1}}"#)
            .expect("overflowing BYOK JSON"),
        1_780_000_000,
    )
    .expect("overflowing BYOK key remains a valid response");
    assert!(
        quota
            .buckets
            .iter()
            .all(|bucket| bucket.label != "BYOK spend")
    );
}

#[test]
fn openrouter_byok_skips_null_and_string_fallback_fields() {
    let quota = parse_openrouter_key_usage(
        serde_json::from_str(
            r#"{"data":{"byok_spend_a":1,"byok_cost_null":null,"byok_usage_text":"1.00"}}"#,
        )
        .expect("mixed BYOK JSON"),
        1_780_000_000,
    )
    .expect("mixed BYOK key");
    let byok = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "BYOK spend")
        .expect("numeric BYOK component remains available");
    assert_eq!(
        byok.used_money,
        Some(jackin_protocol::control::Money::new(100, "USD", 2))
    );
    assert_eq!(byok.used_label.as_deref(), Some("$1"));
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn openrouter_byok_primary_skips_unrepresentable_month_and_selects_week() {
    let quota = parse_openrouter_key_usage(
        serde_json::from_str(r#"{"data":{"byok_usage_monthly":1e-256,"byok_usage_weekly":1}}"#)
            .expect("period BYOK JSON"),
        1_780_000_000,
    )
    .expect("period BYOK key");
    let byok = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "BYOK spend")
        .expect("weekly BYOK fallback");
    assert_eq!(
        byok.used_money,
        Some(jackin_protocol::control::Money::new(100, "USD", 2))
    );
    assert_eq!(byok.used_label.as_deref(), Some("$1"));
}

#[test]
fn openrouter_model_catalog_omission_is_unverified_not_rejection() {
    let catalog = serde_json::json!({"data": [{"id": "openrouter/auto"}]});
    assert!(matches!(
        check_openrouter_model_in_catalog(&catalog, "openrouter/auto"),
        OpenRouterModelCheck::Verified { .. }
    ));
    let stale = check_openrouter_model_in_catalog(&catalog, "acme/new-model");
    assert!(matches!(stale, OpenRouterModelCheck::Unverified { .. }));
    let broken = check_openrouter_model_in_catalog(&serde_json::json!({}), "acme/new-model");
    assert!(matches!(broken, OpenRouterModelCheck::Unverified { .. }));
}

#[test]
fn openrouter_base_url_prefers_api_url_override() {
    assert_eq!(
        openrouter_base_url_from(None, None),
        OPENROUTER_DEFAULT_BASE_URL
    );
    assert_eq!(
        openrouter_base_url_from(Some("https://api.test/"), Some("https://base.test")),
        "https://api.test"
    );
    assert_eq!(
        openrouter_base_url_from(None, Some("https://base.test/")),
        "https://base.test"
    );
    assert_eq!(
        openrouter_base_url_from(Some("  "), Some("https://base.test")),
        "https://base.test"
    );
}

#[test]
fn openrouter_byok_fallback_ignores_caps() {
    let quota = parse_openrouter_key_usage(
        serde_json::json!({
            "data": {
                "usage_monthly": 12.5,
                "byok_limit": 100.0,
                "byok_remaining": 40.0
            }
        }),
        1_780_000_000,
    )
    .expect("byok caps fixture");
    assert!(
        quota
            .buckets
            .iter()
            .all(|bucket| bucket.label != "BYOK spend"),
        "caps never become a spend row"
    );
    let quota = parse_openrouter_key_usage(
        serde_json::json!({
            "data": {"usage_monthly": 12.5, "byok_spend_total": 3.0}
        }),
        1_780_000_000,
    )
    .expect("byok spend fixture");
    let byok = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "BYOK spend")
        .expect("byok row");
    assert_eq!(byok.used_label.as_deref(), Some("$3"));
}

#[test]
fn openrouter_omitted_tier_flag_is_unknown_plan() {
    let quota = parse_openrouter_key_usage(
        serde_json::json!({"data": {"usage_monthly": 1.0}}),
        1_780_000_000,
    )
    .expect("tierless fixture");
    assert_eq!(quota.plan_label, None);
}

#[test]
fn openrouter_raw_key_preserves_exact_money_and_snapshot_handoff() {
    let raw = r#"{"data":{"limit":0.001,"limit_remaining":0.0009,"usage_monthly":0.100000000000000001,"is_free_tier":false}}"#;
    let payload =
        serde_json::value::RawValue::from_string(raw.to_owned()).expect("valid raw key JSON");
    let quota =
        parse_fetched_openrouter_key_quota(payload, 1_780_000_000).expect("raw snapshot payload");

    let cap = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Key Limit")
        .expect("cap row");
    assert_eq!(cap.limit_label.as_deref(), Some("$0.001"));
    assert_eq!(cap.used_label.as_deref(), Some("$0.0001"));
    assert_eq!(
        cap.limit_money,
        Some(jackin_protocol::control::Money::new(10, "USD", 4))
    );
    assert_eq!(
        cap.used_money,
        Some(jackin_protocol::control::Money::new(1, "USD", 4))
    );
    let month = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Spent this month")
        .expect("monthly row");
    assert_eq!(month.used_label.as_deref(), Some("$0.100000000000000001"));
    assert_eq!(
        month.used_money,
        Some(jackin_protocol::control::Money::new(
            100_000_000_000_000_001,
            "USD",
            18
        ))
    );
}

#[test]
fn openrouter_raw_byok_preserves_dynamic_numeric_fallbacks_and_ignores_other_values() {
    let quota = parse_openrouter_key_usage_raw(
        r#"{"data":{"BYOK_COST_first":0.001,"my_Byok_Spend":0.0009,"byok_null":null,"byok_text":"5.00","byok_array":[3],"byok_flag":false}}"#,
        1_780_000_000,
    )
    .expect("mixed dynamic BYOK fields");
    let byok = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "BYOK spend")
        .expect("numeric BYOK row");
    assert_eq!(byok.used_label.as_deref(), Some("$0.0019"));
    assert_eq!(
        byok.used_money,
        Some(jackin_protocol::control::Money::new(19, "USD", 4))
    );
}

#[test]
fn openrouter_raw_byok_omits_incomplete_groups_and_selects_first_convertible_period() {
    for json in [
        r#"{"data":{"byok_spend_a":1,"byok_cost_b":1e-256}}"#,
        r#"{"data":{"byok_spend_a":1,"byok_cost_b":1e30}}"#,
        r#"{"data":{"byok_spend_a":9223372036854775807,"byok_cost_b":1}}"#,
    ] {
        let quota = parse_openrouter_key_usage_raw(json, 1_780_000_000)
            .expect("incomplete numeric group is a valid response");
        assert!(
            quota
                .buckets
                .iter()
                .all(|bucket| bucket.label != "BYOK spend")
        );
    }

    let quota = parse_openrouter_key_usage_raw(
        r#"{"data":{"byok_usage_monthly":1e-256,"byok_usage_weekly":1}}"#,
        1_780_000_000,
    )
    .expect("period fallback key");
    let byok = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "BYOK spend")
        .expect("weekly BYOK row");
    assert_eq!(byok.used_label.as_deref(), Some("$1"));
}

#[test]
fn openrouter_raw_parser_preserves_duplicate_last_wins_and_typed_validation() {
    let quota = parse_openrouter_key_usage_raw(
        r#"{"data":{"usage_monthly":1},"data":{"usage_monthly":1.25,"usage_monthly":2.5}}"#,
        1_780_000_000,
    )
    .expect("duplicate keys use the final value");
    let month = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Spent this month")
        .expect("monthly row");
    assert_eq!(month.used_label.as_deref(), Some("$2.50"));
    assert_eq!(
        month.used_money,
        Some(jackin_protocol::control::Money::new(250, "USD", 2))
    );

    for json in [
        "{}",
        r#"{"data":null}"#,
        r#"{"data":{"usage_monthly":"1"}}"#,
    ] {
        assert!(
            parse_openrouter_key_usage_raw(json, 1_780_000_000).is_err(),
            "invalid typed response should fail: {json}"
        );
    }

    let quota = parse_openrouter_key_usage_raw(
        r#"{"data":{"usage_monthly":1.25,"usage_daily":null,"byok_cost_null":null,"byok_usage_text":"4.00"}}"#,
        1_780_000_000,
    )
    .expect("nullable known and nonnumeric dynamic fields are skipped");
    assert_eq!(
        quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Spent this month")
            .and_then(|bucket| bucket.used_label.as_deref()),
        Some("$1.25")
    );
    assert!(
        quota
            .buckets
            .iter()
            .all(|bucket| bucket.label != "BYOK spend")
    );

    let quota = parse_openrouter_key_usage_raw(
        r#"{"data":{"usage_monthly":0e-400,"unknown_number":1e400}}"#,
        1_780_000_000,
    )
    .expect("zero exponent and unknown numeric field are accepted");
    let month = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Spent this month")
        .expect("zero monthly row");
    assert_eq!(month.used_label.as_deref(), Some("$0"));
    assert_eq!(
        month.used_money,
        Some(jackin_protocol::control::Money::new(0, "USD", 2))
    );

    for json in [
        r#"{"data":{"usage_monthly":1e-400}}"#,
        r#"{"data":{"usage_monthly":1e400}}"#,
    ] {
        let quota = parse_openrouter_key_usage_raw(json, 1_780_000_000)
            .expect("valid f64-shaped exponent is omitted from exact money");
        assert!(
            quota
                .buckets
                .iter()
                .all(|bucket| bucket.label != "Spent this month")
        );
    }
}

#[test]
fn openrouter_value_parser_uses_the_precision_retained_by_its_number() {
    let value: serde_json::Value =
        serde_json::from_str(r#"{"data":{"usage_monthly":0.100000000000000001}}"#)
            .expect("value key JSON");
    let retained_number = value
        .get("data")
        .and_then(|data| data.get("usage_monthly"))
        .and_then(serde_json::Value::as_number)
        .map(ToString::to_string)
        .expect("monthly number");
    let quota = parse_openrouter_key_usage(value, 1_780_000_000).expect("value parser");
    let month = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Spent this month")
        .expect("monthly row");
    match retained_number.as_str() {
        "0.1" => {
            assert_eq!(month.used_label.as_deref(), Some("$0.10"));
            assert_eq!(
                month.used_money,
                Some(jackin_protocol::control::Money::new(10, "USD", 2))
            );
        }
        "0.100000000000000001" => {
            assert_eq!(month.used_label.as_deref(), Some("$0.100000000000000001"));
            assert_eq!(
                month.used_money,
                Some(jackin_protocol::control::Money::new(
                    100_000_000_000_000_001,
                    "USD",
                    18
                ))
            );
        }
        other => panic!("unexpected retained number spelling: {other}"),
    }
}

#[test]
fn openrouter_raw_and_value_lanes_preserve_key_data_sequence_shapes() {
    for (json, expected_plan) in [
        (r#"{"data":[]}"#, None),
        (r#"{"data":[1.25]}"#, None),
        (r#"{"data":[null,null,null,2.5]}"#, None),
        (
            r#"{"data":[null,null,null,null,null,null,true]}"#,
            Some("Free tier"),
        ),
    ] {
        let raw = parse_openrouter_key_usage_raw(json, 1_780_000_000);
        let value = serde_json::from_str::<serde_json::Value>(json)
            .map_err(|error| error.to_string())
            .and_then(|value| parse_openrouter_key_usage(value, 1_780_000_000));
        assert_eq!(raw.is_ok(), value.is_ok(), "input: {json}");
        let raw = raw.expect("compatible key-data sequence");
        let value = value.expect("compatible Value key-data sequence");
        assert_eq!(
            quota_projection(&raw),
            quota_projection(&value),
            "input: {json}"
        );
        assert!(
            raw.buckets.is_empty(),
            "sequence fields are not named money fields"
        );
        assert_eq!(raw.plan_label.as_deref(), expected_plan, "input: {json}");
    }

    for json in [
        r#"{"data":null}"#,
        r#"{"data":"key"}"#,
        r#"{"data":1}"#,
        r#"{"data":true}"#,
        r#"{"data":["1"]}"#,
        r#"{"data":[null,"1"]}"#,
        r#"{"data":[null,null,null,null,null,null,0]}"#,
        r#"{"data":[null,null,null,null,null,null,true,false]}"#,
    ] {
        let raw = parse_openrouter_key_usage_raw(json, 1_780_000_000);
        let value = serde_json::from_str::<serde_json::Value>(json)
            .map_err(|error| error.to_string())
            .and_then(|value| parse_openrouter_key_usage(value, 1_780_000_000));
        assert_eq!(raw.is_ok(), value.is_ok(), "input: {json}");
        assert!(raw.is_err(), "invalid key-data shape should fail: {json}");
    }

    for json in [
        r#"{"data":[1e400]}"#,
        r#"{"data":[null,null,null,1e400]}"#,
        r#"{"data":[null,null,null,0e400]}"#,
        r#"{"data":[null,null,null,1e-400]}"#,
    ] {
        let raw = parse_openrouter_key_usage_raw(json, 1_780_000_000)
            .expect("extreme positional f64 value remains typed-valid");
        assert!(
            raw.buckets.is_empty(),
            "sequence fields are not named money fields"
        );
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json) {
            let value = parse_openrouter_key_usage(value, 1_780_000_000)
                .expect("Value sequence has the same typed acceptance");
            assert_eq!(
                quota_projection(&raw),
                quota_projection(&value),
                "input: {json}"
            );
        }
    }
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn openrouter_raw_and_value_lanes_match_typed_extreme_exponent_behavior() {
    for json in [
        r#"{"data":{"usage_monthly":0e-400}}"#,
        r#"{"data":{"usage_monthly":-0e-400}}"#,
        r#"{"data":{"usage_monthly":0e400}}"#,
        r#"{"data":{"usage_monthly":1e-400}}"#,
        r#"{"data":{"usage_monthly":1e400}}"#,
        r#"{"data":{"usage_monthly":1,"unknown_number":1e400}}"#,
    ] {
        let raw = parse_openrouter_key_usage_raw(json, 1_780_000_000);
        let value_json = serde_json::from_str(json).expect("arbitrary precision value JSON");
        let value = parse_openrouter_key_usage(value_json, 1_780_000_000);
        assert_eq!(raw.is_ok(), value.is_ok(), "input: {json}");
        if let (Ok(raw), Ok(value)) = (raw, value) {
            let projection = |quota: &OpenRouterKeyQuota| {
                quota
                    .buckets
                    .iter()
                    .map(|bucket| {
                        (
                            bucket.label.clone(),
                            bucket.used_label.clone(),
                            bucket.limit_label.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(raw.plan_label, value.plan_label, "input: {json}");
            assert_eq!(projection(&raw), projection(&value), "input: {json}");
        }
    }
}

#[test]
fn openrouter_raw_and_value_lanes_match_duplicate_and_byok_projection() {
    for (index, json) in [
        r#"{"data":{"byok_spend_a":1.25,"BYOK_cost_b":2.5,"byok_cost_b":0.5,"byok_usage_text":"5","byok_null":null}}"#,
        r#"{"data":{"byok_spend_a":1,"byok_spend_a":3,"byok_cost_b":2}}"#,
        r#"{"data":{"usage_monthly":1},"data":{"usage_monthly":2}}"#,
    ]
    .into_iter()
    .enumerate()
    {
        let raw = parse_openrouter_key_usage_raw(json, 1_780_000_000);
        let value_json = serde_json::from_str(json).expect("ordinary numeric value JSON");
        let value = parse_openrouter_key_usage(value_json, 1_780_000_000);
        assert_eq!(raw.is_ok(), value.is_ok(), "input: {json}");
        if let (Ok(raw), Ok(value)) = (raw, value) {
            assert_eq!(quota_projection(&raw), quota_projection(&value), "input: {json}");
            match index {
                0 => assert_eq!(
                    raw.buckets
                        .iter()
                        .find(|bucket| bucket.label == "BYOK spend")
                        .and_then(|bucket| bucket.used_label.as_deref()),
                    Some("$4.25")
                ),
                1 => assert_eq!(
                    raw.buckets
                        .iter()
                        .find(|bucket| bucket.label == "BYOK spend")
                        .and_then(|bucket| bucket.used_label.as_deref()),
                    Some("$5")
                ),
                2 => assert_eq!(
                    raw.buckets
                        .iter()
                        .find(|bucket| bucket.label == "Spent this month")
                        .and_then(|bucket| bucket.used_label.as_deref()),
                    Some("$2")
                ),
                _ => unreachable!("three fixed corpora"),
            }
        }
    }
}

#[derive(Debug, PartialEq)]
struct QuotaProjection {
    plan_label: Option<String>,
    buckets: Vec<BucketProjection>,
}

#[derive(Debug, PartialEq)]
struct BucketProjection {
    label: String,
    used_label: Option<String>,
    limit_label: Option<String>,
    used_money: Option<jackin_protocol::control::Money>,
    limit_money: Option<jackin_protocol::control::Money>,
}

fn quota_projection(quota: &OpenRouterKeyQuota) -> QuotaProjection {
    QuotaProjection {
        plan_label: quota.plan_label.clone(),
        buckets: quota
            .buckets
            .iter()
            .map(|bucket| BucketProjection {
                label: bucket.label.clone(),
                used_label: bucket.used_label.clone(),
                limit_label: bucket.limit_label.clone(),
                used_money: bucket.used_money.clone(),
                limit_money: bucket.limit_money.clone(),
            })
            .collect(),
    }
}

#[test]
fn openrouter_snapshot_without_key_needs_login() {
    let view = openrouter_snapshot("opencode", None, 1_780_000_000);
    assert_eq!(view.status, UsageSnapshotStatus::NeedsLogin);
    assert_eq!(view.source, UsageSource::None);
    assert_eq!(view.account.provider_label, "OpenRouter");
}

#[test]
fn openrouter_snapshot_with_base_serves_key_quota_from_canned_api() {
    use std::io::{Read as _, Write as _};

    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).unwrap();
            let body = if String::from_utf8_lossy(&request[..read]).contains("/credits") {
                serde_json::json!({"data": {"total_credits": 10.0, "total_usage": 2.5}}).to_string()
            } else {
                key_fixture().to_string()
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let view = openrouter_snapshot_with_base(
        "opencode",
        Some("fixture-key"),
        &format!("http://{address}"),
        1_780_000_000,
    );
    server.join().unwrap();

    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.account.provider_label, "OpenRouter");
    assert!(
        view.buckets
            .iter()
            .any(|bucket| bucket.label == "Key Limit")
    );
}

#[test]
fn openrouter_snapshot_with_controlled_connection_error_is_error() {
    let (view, rate_limit) = openrouter_snapshot_with_key_fetch(
        "opencode",
        Some("fixture-key"),
        "http://127.0.0.1:40101",
        1_780_000_000,
        |_base_url, _key| {
            Err(ProviderHttpError::Transport(
                "OpenRouter key request failed for http://127.0.0.1:40101/key: connection refused"
                    .to_owned(),
            ))
        },
    );

    assert_eq!(view.status, UsageSnapshotStatus::Error);
    assert_eq!(rate_limit, None);
    assert_eq!(view.account.provider_label, "OpenRouter");
    assert_eq!(
        view.last_error.as_deref(),
        Some("OpenRouter key request failed for http://127.0.0.1:40101/key: connection refused")
    );
}

#[test]
fn openrouter_transport_text_and_url_401_are_not_auth_failures() {
    let failures = [
        ProviderHttpError::Transport(
            "OpenRouter key request failed: transport reported status 401".to_owned(),
        ),
        ProviderHttpError::Transport(
            "OpenRouter key request failed for http://127.0.0.1:40101/key".to_owned(),
        ),
        ProviderHttpError::Decode("OpenRouter key decode failed: payload mentions 401".to_owned()),
    ];

    for failure in &failures {
        let typed = ProviderError::from(failure.clone());
        assert_eq!(
            openrouter_key_error_status(&typed),
            UsageSnapshotStatus::Error,
            "non-HTTP-status failure must not become NeedsLogin: {failure}"
        );
    }
    assert_eq!(
        openrouter_key_error_status(&ProviderError::from(ProviderHttpError::HttpStatus {
            status: 401,
            message: "OpenRouter key HTTP 401 Unauthorized".to_owned(),
            retry_after_seconds: None,
            response_received_at_epoch: None,
        })),
        UsageSnapshotStatus::NeedsLogin
    );
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn openrouter_value_byok_skips_unrepresentable_negative_numeric_components() {
    for json in [
        r#"{"data":{"byok_spend_a":1,"byok_cost_b":-1e-256}}"#,
        r#"{"data":{"byok_cost_b":-1e-256,"byok_spend_a":1}}"#,
        r#"{"data":{"byok_spend_a":-1e-256,"byok_cost_b":1}}"#,
        r#"{"data":{"byok_spend_a":1,"byok_cost_b":-1e30}}"#,
        r#"{"data":{"byok_spend_a":1,"byok_cost_b":-1e400}}"#,
        r#"{"data":{"byok_spend_a":1,"byok_cost_b":-0.01}}"#,
    ] {
        let input = serde_json::from_str(json).expect("BYOK JSON");
        let quota = parse_openrouter_key_usage(input, 1_780_000_000).expect("BYOK key");
        let byok = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "BYOK spend")
            .expect("representable positive fallback remains visible");
        assert_eq!(
            byok.used_money,
            Some(jackin_protocol::control::Money::new(100, "USD", 2))
        );
        assert_eq!(byok.used_label.as_deref(), Some("$1"));
    }

    let all_negative = serde_json::from_str(
        r#"{"data":{"byok_spend_a":-1e-256,"byok_cost_b":-1e30,"byok_cost_c":-0.01}}"#,
    )
    .expect("all-negative BYOK JSON");
    let quota = parse_openrouter_key_usage(all_negative, 1_780_000_000).expect("BYOK key");
    assert!(
        quota
            .buckets
            .iter()
            .all(|bucket| bucket.label != "BYOK spend")
    );

    for token in ["-0", "-0.0e-400", "-0e400"] {
        let json = format!(r#"{{"data":{{"byok_spend_a":{token}}}}}"#);
        let input = serde_json::from_str(&json).expect("negative-zero BYOK JSON");
        let quota = parse_openrouter_key_usage(input, 1_780_000_000).expect("BYOK key");
        let byok = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "BYOK spend")
            .expect("negative numeric zero remains a reported contribution");
        assert_eq!(
            byok.used_money,
            Some(jackin_protocol::control::Money::new(0, "USD", 2))
        );
        assert_eq!(byok.used_label.as_deref(), Some("$0"));
    }
}

#[test]
fn openrouter_raw_byok_skips_unrepresentable_negative_numeric_components() {
    for json in [
        r#"{"data":{"byok_spend_a":1,"byok_cost_b":-1e-256}}"#,
        r#"{"data":{"byok_cost_b":-1e-256,"byok_spend_a":1}}"#,
        r#"{"data":{"byok_spend_a":-1e-256,"byok_cost_b":1}}"#,
        r#"{"data":{"byok_spend_a":1,"byok_cost_b":-1e30}}"#,
        r#"{"data":{"byok_spend_a":1,"byok_cost_b":-1e400}}"#,
        r#"{"data":{"byok_spend_a":1,"byok_cost_b":-0.01}}"#,
    ] {
        let quota = parse_openrouter_key_usage_raw(json, 1_780_000_000).expect("BYOK key");
        let byok = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "BYOK spend")
            .expect("representable positive fallback remains visible");
        assert_eq!(
            byok.used_money,
            Some(jackin_protocol::control::Money::new(100, "USD", 2))
        );
        assert_eq!(byok.used_label.as_deref(), Some("$1"));
    }

    let all_negative = parse_openrouter_key_usage_raw(
        r#"{"data":{"byok_spend_a":-1e-256,"byok_cost_b":-1e30,"byok_cost_c":-0.01}}"#,
        1_780_000_000,
    )
    .expect("all-negative BYOK key");
    assert!(
        all_negative
            .buckets
            .iter()
            .all(|bucket| bucket.label != "BYOK spend")
    );

    for token in ["-0", "-0.0e-400", "-0e400"] {
        let json = format!(r#"{{"data":{{"byok_spend_a":{token}}}}}"#);
        let quota = parse_openrouter_key_usage_raw(&json, 1_780_000_000).expect("BYOK key");
        let byok = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "BYOK spend")
            .expect("negative numeric zero remains a reported contribution");
        assert_eq!(
            byok.used_money,
            Some(jackin_protocol::control::Money::new(0, "USD", 2))
        );
        assert_eq!(byok.used_label.as_deref(), Some("$0"));
    }
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn openrouter_value_cap_minus_zero_preserves_fractional_usage() {
    for remaining in ["0", "-0", "-0.0e-400"] {
        let json = format!(
            r#"{{"data":{{"limit":0.5,"limit_remaining":{remaining},"is_free_tier":false}}}}"#
        );
        let input = serde_json::from_str(&json).expect("fractional cap JSON");
        let quota = parse_openrouter_key_usage(input, 1_780_000_000).expect("fractional cap");
        let cap = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Key Limit")
            .expect("cap row");
        assert_eq!(
            cap.limit_money,
            Some(jackin_protocol::control::Money::new(50, "USD", 2))
        );
        assert_eq!(cap.limit_label.as_deref(), Some("$0.50"));
        assert_eq!(
            cap.used_money,
            Some(jackin_protocol::control::Money::new(50, "USD", 2))
        );
        assert_eq!(cap.used_label.as_deref(), Some("$0.50"));
        assert_eq!(
            cap.remaining_percent
                .map(jackin_protocol::control::CanonicalRemainingPercent::get),
            Some(0.0)
        );
        assert_eq!(
            cap.remaining_percent_origin,
            Some(jackin_protocol::control::PercentObservationOrigin::Derived)
        );
        assert_eq!(
            jackin_usage_provider_core::money_used_percent(cap)
                .map(jackin_protocol::control::RawPercentPoints::get),
            Some(100.0)
        );
    }
}

#[test]
fn openrouter_raw_cap_minus_zero_preserves_fractional_usage() {
    for remaining in ["0", "-0", "-0.0e-400"] {
        let json = format!(
            r#"{{"data":{{"limit":0.5,"limit_remaining":{remaining},"is_free_tier":false}}}}"#
        );
        let quota =
            parse_openrouter_key_usage_raw(&json, 1_780_000_000).expect("fractional raw cap");
        let cap = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Key Limit")
            .expect("cap row");
        assert_eq!(
            cap.limit_money,
            Some(jackin_protocol::control::Money::new(50, "USD", 2))
        );
        assert_eq!(cap.limit_label.as_deref(), Some("$0.50"));
        assert_eq!(
            cap.used_money,
            Some(jackin_protocol::control::Money::new(50, "USD", 2))
        );
        assert_eq!(cap.used_label.as_deref(), Some("$0.50"));
        assert_eq!(
            cap.remaining_percent
                .map(jackin_protocol::control::CanonicalRemainingPercent::get),
            Some(0.0)
        );
        assert_eq!(
            cap.remaining_percent_origin,
            Some(jackin_protocol::control::PercentObservationOrigin::Derived)
        );
        assert_eq!(
            jackin_usage_provider_core::money_used_percent(cap)
                .map(jackin_protocol::control::RawPercentPoints::get),
            Some(100.0)
        );
    }

    let quota = parse_openrouter_key_usage_raw(
        r#"{"data":{"limit":0.001,"limit_remaining":0,"is_free_tier":false}}"#,
        1_780_000_000,
    )
    .expect("subcent raw cap");
    let cap = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Key Limit")
        .expect("subcent cap row");
    assert_eq!(
        cap.used_money,
        Some(jackin_protocol::control::Money::new(1, "USD", 3))
    );
    assert_eq!(cap.used_label.as_deref(), Some("$0.001"));
}
