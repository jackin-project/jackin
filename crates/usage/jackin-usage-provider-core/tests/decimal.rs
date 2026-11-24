// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{Decimal, DecimalError};
use serde_json::json;

#[path = "support/promptly.rs"]
mod promptly_support;
use promptly_support::promptly;

type Outcome = Result<String, DecimalError>;

fn strict(text: &str) -> Outcome {
    Decimal::parse_number(text).map(|amount| amount.to_string())
}

fn lenient(text: &str) -> Outcome {
    Decimal::parse_text(text).map(|amount| amount.to_string())
}

fn number(text: &str) -> Result<Decimal, DecimalError> {
    Decimal::parse_number(text)
}

#[test]
fn json_number_lexemes_normalize_to_plain_decimals() {
    for (lexeme, plain) in [
        ("0", "0"),
        ("-0", "0"),
        ("0.0", "0"),
        ("0.000e-5", "0"),
        ("0e999999999999", "0"),
        ("1.50", "1.5"),
        ("0.004", "0.004"),
        ("-0.25", "-0.25"),
        ("1e3", "1000"),
        ("1E+3", "1000"),
        ("1e+21", "1000000000000000000000"),
        ("1.5e-7", "0.00000015"),
        ("1e-07", "0.0000001"),
        ("0.1e1", "1"),
        ("100e-2", "1"),
        ("0.10000000000000001", "0.10000000000000001"),
        (
            "12345678901234567890.123456789",
            "12345678901234567890.123456789",
        ),
        (
            "1.1102230246251565e-16",
            "0.00000000000000011102230246251565",
        ),
    ] {
        assert_eq!(strict(lexeme), Ok(plain.to_owned()), "{lexeme}");
    }
}

#[test]
fn the_number_grammar_is_the_json_grammar() {
    for lexeme in [
        "", "-", "+1", "01", "-01", "1.", ".5", "-.5", "1e", "1e+", "--1", "1.2.3", "0x10", "NaN",
        "Infinity", "1 ", " 1", "1,5", "1_0", "\u{661}",
    ] {
        assert_eq!(strict(lexeme), Err(DecimalError::Invalid), "{lexeme:?}");
    }
}

#[test]
fn the_text_grammar_trims_and_accepts_plain_decimal_forms() {
    for (text, plain) in [
        (" 7.5 ", "7.5"),
        ("+1", "1"),
        (".5", "0.5"),
        ("5.", "5"),
        ("1e3", "1000"),
        ("-.5e1", "-5"),
        ("007", "7"),
    ] {
        assert_eq!(lenient(text), Ok(plain.to_owned()), "{text:?}");
    }
    for text in [
        "", " ", "n/a", "NaN", "inf", "-inf", "infinity", "1e", "1,5", "$1", "0x1", ".", "+", "1 2",
    ] {
        assert_eq!(lenient(text), Err(DecimalError::Invalid), "{text:?}");
    }
}

#[test]
fn thirty_eight_digits_and_thirty_eight_places_are_the_bounds() {
    let nines = "9".repeat(38);
    for lexeme in [
        nines.as_str(),
        &format!("0.{nines}"),
        &format!("-{nines}"),
        "1e37",
        "9.9e37",
        "1e-38",
        "100e-40",
        &format!("1.{}", "0".repeat(60)),
    ] {
        assert_eq!(number(lexeme).map(|_| ()), Ok(()), "{lexeme}");
    }
    for lexeme in [
        "100000000000000000000000000000000000000",
        "123456789012345678901234567890123456789",
        "1e38",
        "10e37",
        "-1e38",
        "1e-39",
        "1.1e-38",
        "0.000000000000000000000000000000000000001",
        "1e99999999999999999999999999999",
        "1e-99999999999999999999999999999",
        "1e999999999999",
        &format!("{nines}0"),
        &format!("0.{nines}9"),
    ] {
        assert_eq!(number(lexeme), Err(DecimalError::OutOfRange), "{lexeme}");
    }
    assert_eq!(
        strict("1e-38"),
        Ok("0.00000000000000000000000000000000000001".to_owned())
    );
}

#[test]
fn enormous_input_is_refused_or_zeroed_without_expanding_it() {
    let found = promptly(|| {
        let digits = "0".repeat(1_000_000);
        (
            number(&format!("1{digits}")),
            number(&format!("0.{digits}1")),
            number(&format!("0.{digits}")),
            number(&format!("1e{}", "9".repeat(1_000_000))),
            number(&format!("0e{}", "9".repeat(1_000_000))),
        )
    });
    assert_eq!(
        found,
        Ok((
            Err(DecimalError::OutOfRange),
            Err(DecimalError::OutOfRange),
            Ok(Decimal::ZERO),
            Err(DecimalError::OutOfRange),
            Ok(Decimal::ZERO),
        ))
    );
}

#[test]
fn addition_and_subtraction_are_exact_across_scales() -> Result<(), DecimalError> {
    let add =
        |left: &str, right: &str| Ok::<_, DecimalError>(number(left)?.checked_add(&number(right)?));
    let sub =
        |left: &str, right: &str| Ok::<_, DecimalError>(number(left)?.checked_sub(&number(right)?));
    assert_eq!(
        add("0.1", "0.2")?.map(|sum| sum.to_string()),
        Some("0.3".to_owned())
    );
    assert_eq!(add("0.25", "0.75")?, Some(number("1")?));
    assert_eq!(
        add("0.25", "0.75")?.map(|sum| sum.to_string()),
        Some("1".to_owned())
    );
    assert_eq!(sub("1.5", "0.5")?, Some(number("1")?));
    assert_eq!(
        sub("17.5", "9.9999999999999999999999999999999999999")?.map(|left| left.to_string()),
        Some("7.5000000000000000000000000000000000001".to_owned())
    );
    assert_eq!(
        add("-17.5", "9.9999999999999999999999999999999999999")?.map(|sum| sum.to_string()),
        Some("-7.5000000000000000000000000000000000001".to_owned())
    );
    assert_eq!(
        add("1.5", "0.0000000000000000000000001")?.map(|sum| sum.to_string()),
        Some("1.5000000000000000000000001".to_owned())
    );
    assert_eq!(
        sub("100000000000.000001", "100000000000")?.map(|left| left.to_string()),
        Some("0.000001".to_owned())
    );
    assert_eq!(
        sub("1", "3")?.map(|left| left.to_string()),
        Some("-2".to_owned())
    );
    assert_eq!(sub("0.5", "0.50")?, Some(Decimal::ZERO));
    assert_eq!(
        sub("1", "1e-38")?.map(|left| left.to_string()),
        Some("0.99999999999999999999999999999999999999".to_owned())
    );
    assert_eq!(sub("10", "1e-38")?, None);
    assert_eq!(add("99999999999999999999999999999999999999", "1")?, None);
    assert_eq!(sub("-99999999999999999999999999999999999999", "1")?, None);
    assert_eq!(add("1e30", "1e-30")?, None);
    Ok(())
}

#[test]
fn ordering_ignores_scale_and_follows_sign() -> Result<(), DecimalError> {
    let ordered = [
        "-2",
        "-1.5",
        "-0.0000001",
        "0",
        "0.0000001",
        "0.1",
        "0.10000000000000001",
        "1",
        "10",
        "1e30",
    ];
    let mut shuffled: Vec<Decimal> = [
        "10",
        "0.1",
        "-1.5",
        "1e30",
        "0",
        "-2",
        "1",
        "0.0000001",
        "0.10000000000000001",
        "-0.0000001",
    ]
    .into_iter()
    .map(number)
    .collect::<Result<_, _>>()?;
    shuffled.sort();
    let expected: Vec<Decimal> = ordered.into_iter().map(number).collect::<Result<_, _>>()?;
    assert_eq!(shuffled, expected);
    assert_eq!(number("1.0")?, number("1.00")?);
    assert_eq!(number("-0")?, number("0")?);
    assert!(number("0.1")? < number("0.10000000000000001")?);
    assert!(number("-0.1")? > number("-0.10000000000000001")?);
    Ok(())
}

#[test]
fn ratios_and_floats_follow_the_value() -> Result<(), DecimalError> {
    assert_eq!(number("0.0006")?.ratio(&number("0.001")?), Some(0.6));
    assert_eq!(number("0.01")?.ratio(&number("0.03")?), Some(1_f64 / 3_f64));
    assert_eq!(
        number("-0.01")?.ratio(&number("0.03")?),
        Some(-1_f64 / 3_f64)
    );
    assert_eq!(
        number("-0.01")?.ratio(&number("-0.03")?),
        Some(1_f64 / 3_f64)
    );
    assert_eq!(number("1")?.ratio(&Decimal::ZERO), None);
    assert_eq!(number("0.1")?.to_f64().to_bits(), 0.1_f64.to_bits());
    assert_eq!(number("-12.5")?.to_f64().to_bits(), (-12.5_f64).to_bits());
    let tiny = number("1e-38")?.ratio(&number("99999999999999999999999999999999999999")?);
    assert!(
        tiny.is_some_and(|ratio| ratio > 0.0 && ratio < 1e-70),
        "{tiny:?}"
    );
    Ok(())
}

#[test]
fn fixed_pads_to_the_places_and_never_drops_a_digit() -> Result<(), DecimalError> {
    for (lexeme, places, text) in [
        ("0.004", 2, "0.004"),
        ("1.5", 2, "1.50"),
        ("5", 2, "5.00"),
        ("5", 0, "5"),
        ("-0.25", 2, "-0.25"),
        ("-0.5", 2, "-0.50"),
        ("0", 2, "0.00"),
        ("12.34", 0, "12.34"),
        ("0.30000000000000004", 2, "0.30000000000000004"),
        (
            "99999999999999999999999999999999999999",
            2,
            "99999999999999999999999999999999999999.00",
        ),
    ] {
        assert_eq!(number(lexeme)?.fixed(places), text, "{lexeme} at {places}");
    }
    Ok(())
}

#[test]
fn a_json_value_is_read_as_a_number_or_as_text() {
    assert_eq!(
        Decimal::from_json(&json!(1.5)).map(|amount| amount.to_string()),
        Ok("1.5".to_owned())
    );
    assert_eq!(
        Decimal::from_json(&json!(42)).map(|amount| amount.to_string()),
        Ok("42".to_owned())
    );
    assert_eq!(
        Decimal::from_json(&json!("2.5e-3")).map(|amount| amount.to_string()),
        Ok("0.0025".to_owned())
    );
    assert_eq!(
        Decimal::from_json(&json!(1e-7)).map(|amount| amount.to_string()),
        Ok("0.0000001".to_owned())
    );
    assert_eq!(Decimal::from_json(&json!(null)), Err(DecimalError::Invalid));
    assert_eq!(Decimal::from_json(&json!(true)), Err(DecimalError::Invalid));
    assert_eq!(
        Decimal::from_json(&json!("abc")),
        Err(DecimalError::Invalid)
    );
    assert_eq!(
        Decimal::from_json(&json!(1e40)),
        Err(DecimalError::OutOfRange)
    );
}

#[test]
fn only_a_whole_amount_within_the_integers_is_a_count() -> Result<(), DecimalError> {
    assert_eq!(number("39")?.to_i64(), Some(39));
    assert_eq!(number("-5")?.to_i64(), Some(-5));
    assert_eq!(number("39.5")?.to_i64(), None);
    assert_eq!(number("9223372036854775808")?.to_i64(), None);
    assert_eq!(number("1e30")?.to_i64(), None);
    Ok(())
}

#[test]
fn the_sign_helpers_and_the_zero_clamp_agree() -> Result<(), DecimalError> {
    let negative = number("-0.001")?;
    let positive = number("0.001")?;
    assert!(negative.is_negative() && !negative.is_positive() && !negative.is_zero());
    assert!(positive.is_positive() && !positive.is_negative());
    assert!(Decimal::ZERO.is_zero() && !Decimal::ZERO.is_positive());
    assert_eq!(negative.clamped_at_zero(), Decimal::ZERO);
    assert_eq!(positive.clamped_at_zero(), positive);
    Ok(())
}
