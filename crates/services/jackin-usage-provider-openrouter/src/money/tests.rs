// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn raw(raw: &str) -> Box<RawValue> {
    RawValue::from_string(raw.to_owned()).expect("valid raw JSON")
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
fn aligned_zero_keeps_one_magnitude_digit_at_every_scale() {
    let zero = Decimal::from_raw(&raw("-0")).expect("negative zero decimal");
    for scale in 0..=u8::MAX {
        let aligned = zero.aligned(scale).expect("aligned zero");
        assert_eq!(aligned.len, 1);
        assert_eq!(aligned.scale(), scale);
        assert!(!aligned.is_negative());
        assert_eq!(aligned.digits[0], 0);
    }
}

#[test]
fn exact_subtraction_preserves_cap_minus_zero_and_saturates_zero_minus_cap() {
    let cap = Decimal::from_raw(&raw("0.5")).expect("cap decimal");
    let zero = Decimal::from_raw(&raw("0")).expect("zero decimal");
    let negative_zero = Decimal::from_raw(&raw("-0.0e-400")).expect("negative zero decimal");

    assert_eq!(
        cap.saturating_sub(&zero)
            .and_then(|value| value.money_at_scale(2)),
        Some(Money::new(50, "USD", 2))
    );
    assert_eq!(
        cap.saturating_sub(&negative_zero)
            .and_then(|value| value.money_at_scale(2)),
        Some(Money::new(50, "USD", 2))
    );
    assert_eq!(
        zero.saturating_sub(&cap)
            .and_then(|value| value.money_at_scale(2)),
        Some(Money::new(0, "USD", 2))
    );
    assert_eq!(
        cap.saturating_sub(&cap)
            .and_then(|value| value.money_at_scale(2)),
        Some(Money::new(0, "USD", 2))
    );

    let subcent = Decimal::from_raw(&raw("0.001")).expect("subcent cap");
    assert_eq!(
        subcent
            .saturating_sub(&zero)
            .and_then(|value| value.money_at_scale(3)),
        Some(Money::new(1, "USD", 3))
    );
}
