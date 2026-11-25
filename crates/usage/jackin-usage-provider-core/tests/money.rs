// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{Decimal, DecimalError, Money};

fn exact(lexeme: &str, places: u32, unit: &str) -> Result<Money, DecimalError> {
    Money::exact(Decimal::parse_number(lexeme)?, places, unit).ok_or(DecimalError::OutOfRange)
}

#[test]
fn minor_unit_money_keeps_its_fixed_text() {
    assert_eq!(
        Money::new(1234, 2, "USD").map(|money| money.text()),
        Some("12.34".to_owned())
    );
    assert_eq!(
        Money::new(-5, 2, "USD").map(|money| money.text()),
        Some("-0.05".to_owned())
    );
    assert_eq!(
        Money::new(7, 0, "resets").map(|money| money.text()),
        Some("7".to_owned())
    );
    assert_eq!(
        Money::new(1, 9, "USD").map(|money| money.text()),
        Some("0.000000001".to_owned())
    );
    assert_eq!(
        Money::new(i64::MIN, 2, "USD").map(|money| money.text()),
        Some("-92233720368547758.08".to_owned())
    );
    assert!(Money::new(1, 10, "USD").is_none());
    assert_eq!(Money::cents(1, "USD").text(), "0.01");
    assert_eq!(Money::cents(0, "USD").text(), "0.00");
}

#[test]
fn exact_money_shows_every_digit_and_pads_to_its_places() -> Result<(), DecimalError> {
    assert_eq!(exact("0.004", 2, "USD")?.text(), "0.004");
    assert_eq!(exact("1.5", 2, "USD")?.text(), "1.50");
    assert_eq!(
        exact("0.30000000000000004", 2, "USD")?.text(),
        "0.30000000000000004"
    );
    assert_eq!(exact("1", 9, "USD")?.text(), "1.000000000");
    assert_eq!(exact("1", 10, "USD"), Err(DecimalError::OutOfRange));
    assert_eq!(exact("1.5", 2, "USD")?, exact("1.50", 2, "USD")?);
    assert_eq!(exact("1.5", 2, "USD")?.unit(), "USD");
    Ok(())
}

#[test]
fn rounded_float_money_still_rounds_to_its_places() {
    assert_eq!(
        Money::from_f64(0.004, 2, "USD").map(|money| money.text()),
        Some("0.00".to_owned())
    );
    assert_eq!(
        Money::from_f64(12.345, 2, "USD").map(|money| money.text()),
        Some("12.35".to_owned())
    );
    assert!(Money::from_f64(1e300, 2, "USD").is_none());
    assert!(Money::from_f64(f64::NAN, 2, "USD").is_none());
}

#[test]
fn subtraction_needs_the_same_unit_and_places_but_not_the_same_scale() -> Result<(), DecimalError> {
    let left = exact("1.5", 2, "USD")?.minus(&exact("0.004", 2, "USD")?);
    assert_eq!(left.map(|money| money.text()), Some("1.496".to_owned()));
    assert!(
        exact("1", 2, "USD")?
            .minus(&exact("1", 2, "EUR")?)
            .is_none()
    );
    assert!(
        exact("1", 2, "USD")?
            .minus(&exact("1", 3, "USD")?)
            .is_none()
    );
    let huge = exact("99999999999999999999999999999999999999", 2, "USD")?;
    let negative = exact("-99999999999999999999999999999999999999", 2, "USD")?;
    assert!(huge.minus(&negative).is_none());
    assert_eq!(
        exact("0.001", 2, "USD")?
            .minus(&exact("0.0006", 2, "USD")?)
            .map(|money| money.text()),
        Some("0.0004".to_owned())
    );
    Ok(())
}

#[test]
fn the_zero_clamp_and_the_sign_test_follow_the_amount() -> Result<(), DecimalError> {
    let negative = exact("-2.5", 2, "USD")?;
    assert_eq!(negative.clamped_at_zero().text(), "0.00");
    assert!(!negative.is_positive());
    assert!(exact("0.0000001", 2, "USD")?.is_positive());
    assert!(!exact("0", 2, "USD")?.is_positive());
    Ok(())
}

#[test]
fn a_percentage_of_a_whole_matches_the_integer_ratio() -> Result<(), DecimalError> {
    let part = Money::cents(600, "USD");
    let whole = Money::cents(1000, "USD");
    assert_eq!(part.percent_of(&whole), Some(600_f64 / 1000_f64 * 100.0));
    assert_eq!(
        exact("0.0006", 2, "USD")?.percent_of(&exact("0.001", 2, "USD")?),
        Some(60.0)
    );
    assert_eq!(
        Money::cents(1, "USD").percent_of(&Money::cents(3, "USD")),
        Some(1_f64 / 3_f64 * 100.0)
    );
    assert_eq!(part.percent_of(&Money::cents(0, "USD")), None);
    assert_eq!(part.percent_of(&Money::cents(-1000, "USD")), None);
    Ok(())
}
