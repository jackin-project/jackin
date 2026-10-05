// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Exact OpenRouter decimal amounts. Requires serde_json/arbitrary_precision.

use jackin_protocol::control::Money;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MoneyField {
    Unknown,
    Null,
    Known(Money),
    Invalid(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SignedPolicy {
    NonNegative,
    /// Only explicit remaining-allowance fields may carry negative overage.
    Remaining,
}

pub(crate) fn parse_money_field(value: Option<&Value>, policy: SignedPolicy) -> MoneyField {
    match value {
        None => MoneyField::Unknown,
        Some(Value::Null) => MoneyField::Null,
        Some(Value::Number(number)) => match parse_decimal(number.as_str(), policy) {
            Ok(money) => MoneyField::Known(money),
            Err(error) => MoneyField::Invalid(error.to_owned()),
        },
        Some(_) => MoneyField::Invalid("expected a JSON number".to_owned()),
    }
}

/// Scan decimal spelling directly, then normalize before checking the i64
/// coefficient. Work is linear in input length, independent of exponent value.
fn parse_decimal(input: &str, policy: SignedPolicy) -> Result<Money, &'static str> {
    let bytes = input.as_bytes();
    let mut cursor = 0;
    let negative = bytes.first() == Some(&b'-');
    if negative {
        cursor += 1;
    }
    let mantissa_start = cursor;
    match bytes.get(cursor) {
        Some(b'0') => {
            cursor += 1;
            if bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
                return Err("invalid decimal spelling");
            }
        }
        Some(b'1'..=b'9') => {
            cursor += 1;
            while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
                cursor += 1;
            }
        }
        _ => return Err("invalid decimal spelling"),
    }
    let mut fractional_digits = 0_usize;
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let fraction_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        fractional_digits = cursor - fraction_start;
        if fractional_digits == 0 {
            return Err("invalid decimal spelling");
        }
    }
    let mantissa_end = cursor;
    let mut exponent = 0_i32;
    if matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        cursor += 1;
        let exponent_negative = bytes.get(cursor) == Some(&b'-');
        if matches!(bytes.get(cursor), Some(b'-' | b'+')) {
            cursor += 1;
        }
        let exponent_start = cursor;
        while let Some(digit @ b'0'..=b'9') = bytes.get(cursor) {
            exponent = exponent
                .checked_mul(10)
                .and_then(|value| value.checked_add(i32::from(*digit - b'0')))
                .ok_or("decimal exponent exceeds parser bounds")?;
            cursor += 1;
        }
        if cursor == exponent_start {
            return Err("invalid decimal spelling");
        }
        if exponent_negative {
            exponent = -exponent;
        }
    }
    if cursor != bytes.len() {
        return Err("invalid decimal spelling");
    }

    let mantissa = &bytes[mantissa_start..mantissa_end];
    let Some(first_nonzero) = mantissa
        .iter()
        .position(|digit| matches!(digit, b'1'..=b'9'))
    else {
        // Validate exponent first, even for zero; enormous exponent spellings
        // therefore cannot bypass the bounded parser. Negative zero is zero.
        return Ok(Money::new(0, "USD", 0));
    };
    if negative && policy == SignedPolicy::NonNegative {
        return Err("negative monetary amount is invalid for this field");
    }
    let last_nonzero = mantissa
        .iter()
        .rposition(|digit| matches!(digit, b'1'..=b'9'))
        .ok_or("invalid decimal spelling")?;
    let trailing_zeros = mantissa[last_nonzero + 1..]
        .iter()
        .filter(|digit| **digit == b'0')
        .count();
    let scale = i64::try_from(fractional_digits)
        .ok()
        .and_then(|fraction| fraction.checked_sub(i64::from(exponent)))
        .and_then(|scale| scale.checked_sub(i64::try_from(trailing_zeros).ok()?))
        .ok_or("decimal scale exceeds parser bounds")?;

    // Accumulate negatively to admit i64::MIN when remaining is negative.
    let mut coefficient = 0_i64;
    for digit in &mantissa[first_nonzero..=last_nonzero] {
        if *digit == b'.' {
            continue;
        }
        coefficient = coefficient
            .checked_mul(10)
            .and_then(|value| value.checked_sub(i64::from(*digit - b'0')))
            .ok_or("monetary coefficient exceeds i64")?;
    }
    if !negative {
        coefficient = coefficient
            .checked_neg()
            .ok_or("monetary coefficient exceeds i64")?;
    }
    let money_exponent = if scale < 0 {
        // Every nonzero i64 coefficient overflows within 19 decimal shifts.
        // Check that bound before looping; never allocate or compute big powers.
        let shifts = scale
            .checked_neg()
            .ok_or("decimal scale exceeds parser bounds")?;
        if shifts > 18 {
            return Err("monetary coefficient exceeds i64");
        }
        for _ in 0..shifts {
            coefficient = coefficient
                .checked_mul(10)
                .ok_or("monetary coefficient exceeds i64")?;
        }
        0
    } else {
        u8::try_from(scale).map_err(|_| "monetary scale exceeds 255 decimal places")?
    };
    Ok(Money::new(coefficient, "USD", money_exponent))
}

/// Fixed decimal accumulator for a complete sum of nonnegative USD fields.
/// 255 scale places + 19 coefficient digits + at most 20 field-count digits
/// fit within 300 digits on the supported 64-bit platforms. Narrow only after
/// all fractional carries have completed; representability is not associative.
pub(crate) struct MoneySum {
    digits: [u8; 300],
    found: bool,
}

impl MoneySum {
    pub(crate) fn new() -> Self {
        Self {
            digits: [0; 300],
            found: false,
        }
    }

    pub(crate) fn add(&mut self, money: &Money) -> Result<(), &'static str> {
        if money.currency != "USD" {
            return Err("incompatible monetary denomination");
        }
        let mut coefficient =
            u64::try_from(money.amount_minor).map_err(|_| "negative monetary sum component")?;
        let mut offset = 255_usize - usize::from(money.exponent);
        let mut carry = 0_u8;
        while coefficient != 0 || carry != 0 {
            let digit = u8::try_from(coefficient % 10).map_err(|_| "invalid monetary sum digit")?;
            coefficient /= 10;
            let target = self
                .digits
                .get_mut(offset)
                .ok_or("monetary sum exceeds accumulator bounds")?;
            let sum = *target + digit + carry;
            *target = sum % 10;
            carry = sum / 10;
            offset += 1;
        }
        self.found = true;
        Ok(())
    }

    pub(crate) fn finish(&self) -> Result<Option<Money>, &'static str> {
        if !self.found {
            return Ok(None);
        }
        let Some(lowest) = self.digits.iter().position(|digit| *digit != 0) else {
            return Ok(Some(Money::new(0, "USD", 0)));
        };
        let highest = self
            .digits
            .iter()
            .rposition(|digit| *digit != 0)
            .ok_or("invalid monetary sum")?;
        // Scale cannot become negative: integral zeros remain in coefficient.
        let coefficient_start = lowest.min(255);
        let exponent = u8::try_from(255 - coefficient_start)
            .map_err(|_| "monetary sum scale exceeds 255 decimal places")?;
        let mut coefficient = 0_i64;
        for digit in self.digits[coefficient_start..=highest].iter().rev() {
            coefficient = coefficient
                .checked_mul(10)
                .and_then(|value| value.checked_add(i64::from(*digit)))
                .ok_or("monetary sum coefficient exceeds i64")?;
        }
        Ok(Some(Money::new(coefficient, "USD", exponent)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(literal: &str, policy: SignedPolicy) -> MoneyField {
        let value: Value = serde_json::from_str(literal).expect("valid JSON fixture");
        parse_money_field(Some(&value), policy)
    }

    fn known(literal: &str, coefficient: i64, exponent: u8) {
        assert_eq!(
            field(literal, SignedPolicy::NonNegative),
            MoneyField::Known(Money::new(coefficient, "USD", exponent))
        );
    }

    #[test]
    fn preserves_literal_precision_and_normalizes_before_overflow() {
        known("0.001", 1, 3);
        known("0.100000000000000001", 100_000_000_000_000_001, 18);
        known("9223372036854775807.0", i64::MAX, 0);
        known("9223372036854775807000e-3", i64::MAX, 0);
        known("1e-255", 1, 255);
        known("10e-256", 1, 255);
        known(
            "1000000000000000000000000000000000000000000000000e-48",
            1,
            0,
        );
        known("1e18", 1_000_000_000_000_000_000, 0);
    }

    #[test]
    fn rejects_unrepresentable_or_absurd_exponents() {
        for literal in [
            "9223372036854775808",
            "1e-256",
            "1e19",
            "1e2147483647",
            "1e-2147483647",
            "1e999999999999999999999999999999",
            "0e999999999999999999999999999999",
            "0e-999999999999999999999999999999",
        ] {
            assert!(
                matches!(
                    field(literal, SignedPolicy::NonNegative),
                    MoneyField::Invalid(_)
                ),
                "{literal}"
            );
        }
        // Zero has no representational scale, but exponent parsing stays checked.
        known("0e2147483647", 0, 0);
        known("0e-2147483647", 0, 0);
    }

    #[test]
    fn sign_policy_only_preserves_negative_remaining() {
        assert!(matches!(
            field("-1", SignedPolicy::NonNegative),
            MoneyField::Invalid(_)
        ));
        known("-0", 0, 0);
        known("-0.000", 0, 0);
        assert_eq!(
            field("-1.25", SignedPolicy::Remaining),
            MoneyField::Known(Money::new(-125, "USD", 2))
        );
        assert_eq!(
            field("-9223372036854775808.0", SignedPolicy::Remaining),
            MoneyField::Known(Money::new(i64::MIN, "USD", 0))
        );
        assert!(matches!(
            field("-9223372036854775809", SignedPolicy::Remaining),
            MoneyField::Invalid(_)
        ));
    }

    #[test]
    fn unknown_null_and_invalid_are_distinct() {
        assert_eq!(
            parse_money_field(None, SignedPolicy::NonNegative),
            MoneyField::Unknown
        );
        assert_eq!(field("null", SignedPolicy::NonNegative), MoneyField::Null);
        for literal in ["true", "[]", "{}", "\"1.5\""] {
            assert!(matches!(
                field(literal, SignedPolicy::NonNegative),
                MoneyField::Invalid(_)
            ));
        }
    }

    #[test]
    fn rejects_malformed_decimal_spellings() {
        for literal in [
            "", "-", "+1", "01", "1.", ".1", "1e", "1e+", "NaN", "inf", "1 2", "1x",
        ] {
            assert!(
                parse_decimal(literal, SignedPolicy::NonNegative).is_err(),
                "{literal}"
            );
        }
    }
}
