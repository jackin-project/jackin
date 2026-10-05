// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn quota(literal: &str) -> Result<OpenRouterKeyQuota, String> {
    parse_openrouter_key_usage(
        serde_json::from_str(literal).expect("literal JSON"),
        1_780_000_000,
    )
}

fn row<'a>(quota: &'a OpenRouterKeyQuota, label: &str) -> &'a QuotaBucketView {
    quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == label)
        .expect("quota row")
}

#[test]
fn exact_subcent_cap_and_unaligned_remaining() {
    let parsed = quota(
        r#"{"data":{"limit":0.001,"limit_remaining":0.0009,"usage_monthly":0.100000000000000001}}"#,
    )
    .unwrap();
    let cap = row(&parsed, "Key Limit");
    assert_eq!(cap.limit_money, Some(Money::new(1, "USD", 3)));
    assert_eq!(cap.used_money, Some(Money::new(1, "USD", 4)));
    assert_eq!(cap.remaining_money, Some(Money::new(9, "USD", 4)));
    assert_eq!(cap.limit_label.as_deref(), Some("$0.001"));
    assert_eq!(cap.used_label.as_deref(), Some("$0.0001"));
    assert_eq!(cap.remaining_percent, Some(90));
    let month = row(&parsed, "Spent this month");
    assert_eq!(
        month.used_money,
        Some(Money::new(100_000_000_000_000_001, "USD", 18))
    );
    assert_eq!(month.used_label.as_deref(), Some("$0.100000000000000001"));
}

#[test]
fn money_geometry_does_not_erase_tiny_remaining() {
    let parsed = quota(r#"{"data":{"limit":1,"limit_remaining":0.001}}"#).unwrap();
    let cap = row(&parsed, "Key Limit");
    assert_eq!(cap.remaining_percent, Some(0));
    assert_eq!(cap.limit_money, Some(Money::new(1, "USD", 0)));
    assert_eq!(cap.used_money, Some(Money::new(999, "USD", 3)));
}

#[test]
fn zero_cap_keeps_unknown_used_unknown() {
    for literal in [
        r#"{"data":{"limit":0}}"#,
        r#"{"data":{"limit":0,"limit_remaining":null}}"#,
    ] {
        let parsed = quota(literal).unwrap();
        let cap = row(&parsed, "Key Limit");
        assert_eq!(cap.limit_money, Some(Money::new(0, "USD", 0)));
        assert_eq!(cap.used_money, None);
        assert_eq!(cap.used_label, None);
        assert_eq!(cap.remaining_percent, None);
    }
}

#[test]
fn maximum_coefficient_normalizes_before_range_check() {
    let parsed = quota(r#"{"data":{"limit":9223372036854775807.0,"limit_remaining":0}}"#).unwrap();
    let cap = row(&parsed, "Key Limit");
    assert_eq!(cap.limit_money, Some(Money::new(i64::MAX, "USD", 0)));
    assert_eq!(cap.used_money, Some(Money::new(i64::MAX, "USD", 0)));
    assert_eq!(cap.limit_label.as_deref(), Some("$9223372036854775807"));
}

#[test]
fn signed_remaining_preserves_overage() {
    let parsed = quota(r#"{"data":{"limit":1,"limit_remaining":-0.001}}"#).unwrap();
    let cap = row(&parsed, "Key Limit");
    assert_eq!(cap.used_money, Some(Money::new(1001, "USD", 3)));
    assert_eq!(cap.remaining_money, Some(Money::new(-1, "USD", 3)));
    assert_eq!(cap.remaining_percent, Some(0));
    let zero = quota(r#"{"data":{"limit":-0,"limit_remaining":-0}}"#).unwrap();
    assert_eq!(
        row(&zero, "Key Limit").used_money,
        Some(Money::new(0, "USD", 0))
    );
}

#[test]
fn invalid_money_never_becomes_fresh_omission() {
    for literal in [
        r#"{"data":{"limit":-1}}"#,
        r#"{"data":{"usage":-1}}"#,
        r#"{"data":{"usage_daily":-1}}"#,
        r#"{"data":{"usage_weekly":"1"}}"#,
        r#"{"data":{"usage_monthly":true}}"#,
        r#"{"data":{"limit_remaining":{}}}"#,
        r#"{"data":{"limit":9223372036854775808}}"#,
        r#"{"data":{"limit":1e-256}}"#,
        r#"{"data":{"limit":1e999999999999999999999999999999}}"#,
        r#"{"data":{"limit":0e999999999999999999999999999999}}"#,
        r#"{"data":{"limit":1,"limit_remaining":2}}"#,
        r#"{"data":{"limit":9223372036854775807,"limit_remaining":-1}}"#,
        r#"{"data":{"limit":1,"limit_remaining":1e-255}}"#,
    ] {
        assert!(quota(literal).is_err(), "{literal}");
    }
}

#[test]
fn byok_invalid_primary_never_falls_back() {
    for literal in [
        r#"{"data":{"byok_usage_monthly":"1","byok_usage_weekly":1}}"#,
        r#"{"data":{"byok_usage_monthly":-1,"byok_spend_total":1}}"#,
    ] {
        assert!(quota(literal).is_err(), "{literal}");
    }
}

#[test]
fn byok_fallback_sum_is_exact_and_complete() {
    let parsed =
        quota(r#"{"data":{"byok_spend_a":0.001,"byok_cost_b":0.0009,"byok_limit":99}}"#).unwrap();
    assert_eq!(
        row(&parsed, "BYOK spend").used_money,
        Some(Money::new(19, "USD", 4))
    );
    assert_eq!(
        row(&parsed, "BYOK spend").used_label.as_deref(),
        Some("$0.0019")
    );
    let incomplete = quota(r#"{"data":{"byok_spend_a":0,"byok_cost_b":null}}"#).unwrap();
    assert!(
        incomplete
            .buckets
            .iter()
            .all(|bucket| bucket.label != "BYOK spend")
    );
    for literal in [
        r#"{"data":{"byok_spend_a":0.001,"byok_cost_b":"invalid"}}"#,
        r#"{"data":{"byok_spend_a":9223372036854775807,"byok_cost_b":1}}"#,
        r#"{"data":{"byok_spend_a":1,"byok_cost_b":1e-255}}"#,
    ] {
        assert!(quota(literal).is_err(), "{literal}");
    }
}

#[test]
fn known_byok_primary_keeps_precedence_over_fallback_sum() {
    let parsed = quota(r#"{"data":{"byok_usage_monthly":0,"byok_spend_a":9223372036854775807,"byok_cost_b":1,"byok_usage_daily":false,"byok_spend_other":"irrelevant"}}"#).unwrap();
    assert_eq!(
        row(&parsed, "BYOK spend").used_money,
        Some(Money::new(0, "USD", 0))
    );
}

#[test]
fn independent_remaining_survives_missing_or_null_cap() {
    for literal in [
        r#"{"data":{"limit_remaining":0.001}}"#,
        r#"{"data":{"limit":null,"limit_remaining":0.001}}"#,
    ] {
        let parsed = quota(literal).unwrap();
        let cap = row(&parsed, "Key Limit");
        assert_eq!(cap.remaining_money, Some(Money::new(1, "USD", 3)));
        assert_eq!(cap.limit_money, None);
        assert_eq!(cap.used_money, None);
        assert_eq!(cap.limit_label.as_deref(), Some("Unknown"));
        assert_eq!(cap.remaining_percent, None);
    }
    let parsed = quota(r#"{"data":{"limit_remaining":-0.001}}"#).unwrap();
    let cap = row(&parsed, "Key Limit");
    assert_eq!(cap.remaining_money, Some(Money::new(-1, "USD", 3)));
    assert_eq!(cap.used_money, None);
    assert_eq!(cap.remaining_percent, None);
}

#[test]
fn byok_null_primary_uses_next_documented_spend_period() {
    let parsed = quota(r#"{"data":{"byok_usage_monthly":null,"byok_usage_weekly":0.0009,"byok_usage_daily":"irrelevant","byok_spend_total":2}}"#).unwrap();
    assert_eq!(
        row(&parsed, "BYOK spend").used_money,
        Some(Money::new(9, "USD", 4))
    );
}

#[test]
fn byok_sum_narrows_only_after_all_fractional_carries() {
    let parsed = quota(
        r#"{"data":{"byok_spend_a":9223372036854775806,"byok_spend_b":0.1,"byok_spend_c":0.9}}"#,
    )
    .unwrap();
    assert_eq!(
        row(&parsed, "BYOK spend").used_money,
        Some(Money::new(i64::MAX, "USD", 0))
    );

    // Each field is an exact decimal spelling, with no float construction.
    // Sum 9e-1 through 9e-255, then 1e-255: all 255 carries produce one.
    let mut fields = Vec::new();
    for exponent in 1..=255 {
        fields.push(format!("\"byok_spend_part_{exponent:03}\":9e-{exponent}"));
    }
    fields.push("\"byok_spend_tail\":1e-255".to_owned());
    let literal = format!("{{\"data\":{{{}}}}}", fields.join(","));
    let parsed = quota(&literal).unwrap();
    assert_eq!(
        row(&parsed, "BYOK spend").used_money,
        Some(Money::new(1, "USD", 0))
    );
}

#[test]
fn negative_remaining_clamps_geometry_before_i32_percentage_conversion() {
    let parsed = quota(r#"{"data":{"limit":1e-10,"limit_remaining":-1}}"#).unwrap();
    let cap = row(&parsed, "Key Limit");
    assert_eq!(cap.remaining_money, Some(Money::new(-1, "USD", 0)));
    assert_eq!(cap.used_money, Some(Money::new(10_000_000_001, "USD", 10)));
    assert_eq!(cap.remaining_percent, Some(0));
}
