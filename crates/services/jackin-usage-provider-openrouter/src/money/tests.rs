// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(feature = "value-arbitrary-precision")]
fn number(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).expect("number JSON")
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn exact_money_normalizes_before_narrowing_and_preserves_cent_scale() {
    assert_eq!(
        Decimal::from_value(&number("12.50")).and_then(|value| value.to_money()),
        Some(Money::new(1_250, "USD", 2))
    );
    assert_eq!(
        Decimal::from_value(&number("92233720368547758070e-1")).and_then(|value| value.to_money()),
        Some(Money::new(i64::MAX, "USD", 0))
    );
    assert_eq!(
        Decimal::from_value(&number("9223372036854775808")).and_then(|value| value.to_money()),
        None
    );
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn exact_money_accepts_max_scale_and_rejects_scale_overflow() {
    assert_eq!(
        Decimal::from_value(&number("1e-255")).and_then(|value| value.to_money()),
        Some(Money::new(1, "USD", 255))
    );
    assert_eq!(
        Decimal::from_value(&number("1e-256")).and_then(|value| value.to_money()),
        None
    );
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn exact_money_distinguishes_zero_negative_and_non_numeric_values() {
    let zero = Decimal::from_value(&number("0.0000")).expect("zero decimal");
    assert_eq!(zero.to_money(), Some(Money::new(0, "USD", 2)));
    assert!(!zero.is_negative());
    let negative = Decimal::from_value(&number("-0.01")).expect("negative decimal");
    assert!(negative.is_negative());
    assert_eq!(negative.to_money(), Some(Money::new(-1, "USD", 2)));
    assert!(Decimal::from_value(&serde_json::Value::Null).is_none());
    assert!(Decimal::from_value(&serde_json::json!("1.00")).is_none());
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn exact_money_supports_decimal_exponent_and_labels() {
    assert_eq!(
        Decimal::from_value(&number("1e-3")).and_then(|value| value.to_money()),
        Some(Money::new(1, "USD", 3))
    );
    assert_eq!(format_money(&Money::new(1_250, "USD", 2)), "$12.50");
    assert_eq!(format_money(&Money::new(300, "USD", 2)), "$3");
    assert_eq!(format_money(&Money::new(1, "USD", 3)), "$0.001");
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn exact_money_subtraction_aligns_scale_and_clamps_negative_use() {
    let cap = Decimal::from_value(&number("0.001")).expect("cap decimal");
    let remaining = Decimal::from_value(&number("0.0009")).expect("remaining decimal");
    let used = cap.saturating_sub(&remaining).expect("subcent difference");
    assert_eq!(used.money_at_scale(4), Some(Money::new(1, "USD", 4)));
    assert_eq!(cap.money_at_scale(4), Some(Money::new(10, "USD", 4)));
    assert_eq!(
        remaining
            .saturating_sub(&cap)
            .and_then(|value| value.money_at_scale(4)),
        Some(Money::new(0, "USD", 4))
    );
}

#[cfg(feature = "value-arbitrary-precision")]
#[test]
fn exact_sum_carries_fraction_before_narrowing_and_rejects_overflow() {
    let values = [number("9223372036854775806"), number("0.1"), number("0.9")];
    assert_eq!(
        sum_nonnegative(values.iter()),
        Some(Money::new(i64::MAX, "USD", 0))
    );
    let too_large = [number("9223372036854775807"), number("1")];
    assert_eq!(sum_nonnegative(too_large.iter()), None);
}

fn raw(raw: &str) -> Box<RawValue> {
    RawValue::from_string(raw.to_owned()).expect("valid raw JSON")
}

#[test]
fn raw_money_normalizes_before_narrowing_and_preserves_cent_scale() {
    assert_eq!(
        Decimal::from_raw(&raw("12.50")).and_then(|value| value.to_money()),
        Some(Money::new(1_250, "USD", 2))
    );
    assert_eq!(
        Decimal::from_raw(&raw("92233720368547758070e-1")).and_then(|value| value.to_money()),
        Some(Money::new(i64::MAX, "USD", 0))
    );
    assert_eq!(
        Decimal::from_raw(&raw("9223372036854775808")).and_then(|value| value.to_money()),
        None
    );
}

#[test]
fn raw_money_accepts_max_scale_and_rejects_scale_overflow() {
    assert_eq!(
        Decimal::from_raw(&raw("1e-255")).and_then(|value| value.to_money()),
        Some(Money::new(1, "USD", 255))
    );
    assert_eq!(
        Decimal::from_raw(&raw("1e-256")).and_then(|value| value.to_money()),
        None
    );
}

#[test]
fn raw_money_distinguishes_zero_negative_and_non_numeric_values() {
    let zero = Decimal::from_raw(&raw("0.0000")).expect("zero decimal");
    assert_eq!(zero.to_money(), Some(Money::new(0, "USD", 2)));
    assert!(!zero.is_negative());
    let negative = Decimal::from_raw(&raw("-0.01")).expect("negative decimal");
    assert!(negative.is_negative());
    assert_eq!(negative.to_money(), Some(Money::new(-1, "USD", 2)));
    assert!(Decimal::from_raw(&raw("null")).is_none());
    assert!(Decimal::from_raw(&raw("\"1.00\"")).is_none());
    assert!(Decimal::from_raw(&raw("false")).is_none());
    assert!(Decimal::from_raw(&raw("[]")).is_none());
}

#[test]
fn raw_money_supports_decimal_exponent_and_labels() {
    assert_eq!(
        Decimal::from_raw(&raw("1e-3")).and_then(|value| value.to_money()),
        Some(Money::new(1, "USD", 3))
    );
    assert_eq!(format_money(&Money::new(1_250, "USD", 2)), "$12.50");
    assert_eq!(format_money(&Money::new(300, "USD", 2)), "$3");
    assert_eq!(format_money(&Money::new(1, "USD", 3)), "$0.001");
}

#[test]
fn raw_money_subtraction_aligns_scale_and_clamps_negative_use() {
    let cap = Decimal::from_raw(&raw("0.001")).expect("cap decimal");
    let remaining = Decimal::from_raw(&raw("0.0009")).expect("remaining decimal");
    let used = cap.saturating_sub(&remaining).expect("subcent difference");
    assert_eq!(used.money_at_scale(4), Some(Money::new(1, "USD", 4)));
    assert_eq!(cap.money_at_scale(4), Some(Money::new(10, "USD", 4)));
    assert_eq!(
        remaining
            .saturating_sub(&cap)
            .and_then(|value| value.money_at_scale(4)),
        Some(Money::new(0, "USD", 4))
    );
}

#[test]
fn raw_sum_carries_fraction_before_narrowing_and_rejects_overflow() {
    let values = [raw("9223372036854775806"), raw("0.1"), raw("0.9")];
    assert_eq!(
        sum_nonnegative_raw(values.iter().map(Box::as_ref)),
        Some(Money::new(i64::MAX, "USD", 0))
    );
    let too_large = [raw("9223372036854775807"), raw("1")];
    assert_eq!(sum_nonnegative_raw(too_large.iter().map(Box::as_ref)), None);
}

#[test]
fn negative_numeric_classification_preserves_signed_zero() {
    assert!(is_negative_nonzero_number("-1e-256"));
    assert!(is_negative_nonzero_number("-1e400"));
    assert!(is_negative_nonzero_number("-0.01"));
    assert!(!is_negative_nonzero_number("-0"));
    assert!(!is_negative_nonzero_number("-0.0e-400"));
    assert!(!is_negative_nonzero_number("-0e400"));
    assert!(!is_negative_nonzero_number("1"));
}
