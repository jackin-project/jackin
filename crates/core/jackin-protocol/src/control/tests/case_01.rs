// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn money_display_preserves_exact_declared_scale() {
    assert_eq!(
        Money::new(100_000_000_000_000_001, "USD", 18).to_string(),
        "$0.100000000000000001"
    );
    assert_eq!(
        Money::new(-100_000_000_000_000_001, "USD", 18).to_string(),
        "$-0.100000000000000001"
    );
    assert_eq!(Money::new(1_200, "USD", 3).to_string(), "$1.200");
    assert_eq!(Money::new(153, "USD", 0).to_string(), "$153");
    assert_eq!(Money::new(1, "SGD", 3).to_string(), "SGD 0.001");
    assert_eq!(Money::new(-1, "credits", 2).to_string(), "-0.01 credits");
    assert_eq!(Money::new(10, "usd", 2).to_string(), "0.10 usd");
    assert_eq!(Money::new(1, "", 0).to_string(), "1 ");
    assert_eq!(Money::new(0, "USD", 0).to_string(), "$0");
    assert_eq!(Money::new(0, "USD", 2).to_string(), "$0.00");
    assert_eq!(
        Money::new(0, "USD", 255).to_string(),
        format!("$0.{}", "0".repeat(255))
    );
    assert_eq!(
        Money::new(i64::MAX, "USD", 0).to_string(),
        "$9223372036854775807"
    );
    assert_eq!(
        Money::new(i64::MAX, "USD", 2).to_string(),
        "$92233720368547758.07"
    );
    assert_eq!(
        Money::new(i64::MAX, "USD", 18).to_string(),
        "$9.223372036854775807"
    );
    assert_eq!(
        Money::new(i64::MAX, "USD", 255).to_string(),
        format!("$0.{}9223372036854775807", "0".repeat(236))
    );
    assert_eq!(
        Money::new(i64::MIN, "USD", 0).to_string(),
        "$-9223372036854775808"
    );
    assert_eq!(
        Money::new(i64::MIN, "USD", 2).to_string(),
        "$-92233720368547758.08"
    );
    assert_eq!(
        Money::new(i64::MIN, "USD", 18).to_string(),
        "$-9.223372036854775808"
    );
    assert_eq!(
        Money::new(i64::MIN, "USD", 255).to_string(),
        format!("$-0.{}9223372036854775808", "0".repeat(236))
    );
}
#[test]
fn money_compact_rounds_decimal_ties_to_even() {
    assert_eq!(Money::new(149, "USD", 2).format_compact(), "$1");
    assert_eq!(Money::new(-149, "USD", 2).format_compact(), "$-1");
    assert_eq!(Money::new(150, "USD", 2).format_compact(), "$2");
    assert_eq!(Money::new(250, "USD", 2).format_compact(), "$2");
    assert_eq!(Money::new(-150, "USD", 2).format_compact(), "$-2");
    assert_eq!(Money::new(-250, "USD", 2).format_compact(), "$-2");
    assert_eq!(Money::new(50, "USD", 2).format_compact(), "$0");
    assert_eq!(Money::new(-50, "USD", 2).format_compact(), "$-0");
    assert_eq!(Money::new(-1, "USD", 2).format_compact(), "$-0");
    assert_eq!(Money::new(500, "USD", 3).format_compact(), "$0");
    assert_eq!(Money::new(1_500, "USD", 3).format_compact(), "$2");
    assert_eq!(
        Money::new(i64::MAX, "USD", 1).format_compact(),
        "$922337203685477581"
    );
    assert_eq!(
        Money::new(i64::MIN, "USD", 1).format_compact(),
        "$-922337203685477581"
    );
    assert_eq!(Money::new(i64::MIN, "USD", 255).format_compact(), "$-0");
}
#[test]
fn money_raw_percent_handles_large_and_mixed_scale_values() {
    assert_eq!(
        Money::new(100_000_000_000_000_000, "USD", 0).raw_percent_of(&Money::new(
            100_000_000_000_000_000,
            "USD",
            0
        )),
        Some(100)
    );
    assert_eq!(
        Money::new(999_999_999_999_999_999, "USD", 2).raw_percent_of(&Money::new(
            100_000_000_000_000_000,
            "USD",
            0
        )),
        Some(9)
    );
    assert_eq!(
        Money::new(-150, "USD", 2).raw_percent_of(&Money::new(1, "USD", 0)),
        Some(-150)
    );
    assert_eq!(
        Money::new(97, "USD", 2).raw_percent_of(&Money::new(1, "USD", 0)),
        Some(97)
    );
    assert_eq!(
        Money::new(-1, "USD", 2).raw_percent_of(&Money::new(3, "USD", 0)),
        Some(0)
    );
    assert_eq!(
        Money::new(i64::MAX, "USD", 0).raw_percent_of(&Money::new(1, "USD", 0)),
        Some(i32::MAX)
    );
    assert_eq!(
        Money::new(i64::MIN, "USD", 0).raw_percent_of(&Money::new(1, "USD", 0)),
        Some(i32::MIN)
    );
    assert_eq!(
        Money::new(i64::MAX, "USD", 255).raw_percent_of(&Money::new(1, "USD", 0)),
        Some(0)
    );
    assert_eq!(
        Money::new(1, "USD", 0).raw_percent_of(&Money::new(i64::MAX, "USD", 255)),
        Some(i32::MAX)
    );
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
fn money_checked_arithmetic_preserves_exact_values_or_reports_unavailable() {
    let cap = Money::new(100_000_000_000_000_000, "USD", 0);
    let spent = Money::new(999_999_999_999_999_999, "USD", 2);
    assert_eq!(cap.checked_cmp(&spent), Some(Ordering::Greater));
    assert_eq!(
        cap.checked_difference(&spent),
        Some(Money::new(9_000_000_000_000_000_001, "USD", 2))
    );
    assert_eq!(
        Money::new(200, "USD", 2).checked_difference(&Money::new(100, "USD", 2)),
        Some(Money::new(100, "USD", 2))
    );
    assert_eq!(
        Money::new(50, "USD", 2).checked_difference(&Money::new(100, "USD", 2)),
        Some(Money::new(-50, "USD", 2))
    );
    assert_eq!(
        Money::new(i64::MAX, "USD", 1).checked_difference(&Money::new(-3, "USD", 1)),
        Some(Money::new(922_337_203_685_477_581, "USD", 0))
    );
    assert_eq!(
        Money::new(i64::MIN, "USD", 0).checked_difference(&Money::new(0, "USD", 0)),
        Some(Money::new(i64::MIN, "USD", 0))
    );
    assert_eq!(
        Money::new(i64::MAX, "USD", 0).checked_difference(&Money::new(-1, "USD", 0)),
        None
    );
    assert_eq!(
        Money::new(1, "USD", 0).checked_difference(&Money::new(1, "USD", 255)),
        None
    );
    assert_eq!(
        Money::new(i64::MIN, "USD", 0).checked_nonnegative_difference(&Money::new(
            i64::MAX,
            "USD",
            0
        )),
        Some(Money::new(0, "USD", 0))
    );
    assert_eq!(
        Money::new(i64::MAX, "USD", 0).checked_nonnegative_difference(&Money::new(-1, "USD", 0)),
        None
    );
    assert_eq!(
        Money::new(1, "USD", 0).checked_cmp(&Money::new(100, "SGD", 2)),
        None
    );
}
#[test]
fn money_major_amount_preserves_decimal_precision() {
    let cases = [
        (
            Money::new(9_007_199_254_740_993, "USD", 0),
            9_007_199_254_740_993,
        ),
        (
            Money::new(-9_007_199_254_740_993, "USD", 0),
            -9_007_199_254_740_993,
        ),
        (Money::new(149_999_999_999_999_999, "USD", 17), 1),
        (Money::new(-149_999_999_999_999_999, "USD", 17), -1),
        (Money::new(i64::MAX, "USD", 1), 922_337_203_685_477_581),
    ];

    let actual = cases
        .iter()
        .map(|(money, _)| money.major_amount())
        .collect::<Vec<_>>();
    let expected = cases.map(|(_, amount)| amount);
    assert_eq!(actual, expected);
}
#[test]
fn money_major_amount_keeps_ties_away_signs_and_bounds() {
    let cases = [
        (Money::new(5, "USD", 1), 1),
        (Money::new(15, "USD", 1), 2),
        (Money::new(25, "USD", 1), 3),
        (Money::new(-5, "USD", 1), -1),
        (Money::new(-15, "USD", 1), -2),
        (Money::new(-25, "USD", 1), -3),
        (Money::new(14, "USD", 1), 1),
        (Money::new(16, "USD", 1), 2),
        (Money::new(-14, "USD", 1), -1),
        (Money::new(-16, "USD", 1), -2),
        (Money::new(i64::MAX, "USD", 0), i64::MAX),
        (Money::new(i64::MIN, "USD", 0), i64::MIN),
        (Money::new(i64::MAX, "USD", 19), 1),
        (Money::new(i64::MIN, "USD", 19), -1),
        (Money::new(i64::MAX, "USD", 20), 0),
        (Money::new(i64::MIN, "USD", 20), 0),
        (Money::new(i64::MAX, "USD", 255), 0),
        (Money::new(i64::MIN, "USD", 255), 0),
        (Money::new(0, "USD", 0), 0),
        (Money::new(0, "USD", 255), 0),
    ];

    for (money, expected) in cases {
        assert_eq!(money.major_amount(), expected, "{money:?}");
    }
}
