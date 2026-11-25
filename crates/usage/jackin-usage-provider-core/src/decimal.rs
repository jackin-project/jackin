// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::fmt;

use serde_json::Value;

pub const MAX_DIGITS: usize = 38;
pub const MAX_SCALE: u32 = 38;

const EXPONENT_CEILING: i128 = 1 << 40;
const COEFFICIENT_CEILING: u128 = 10_u128.pow(MAX_DIGITS as u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Decimal {
    coefficient: i128,
    scale: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecimalError {
    Invalid,
    OutOfRange,
}

impl Decimal {
    pub const ZERO: Decimal = Decimal {
        coefficient: 0,
        scale: 0,
    };

    pub fn from_parts(coefficient: i128, scale: u32) -> Option<Decimal> {
        let (coefficient, scale) = normalize(coefficient, scale);
        (scale <= MAX_SCALE && coefficient.unsigned_abs() < COEFFICIENT_CEILING)
            .then_some(Decimal { coefficient, scale })
    }

    pub(crate) fn minor(minor: i64, scale: u32) -> Decimal {
        let (coefficient, scale) = normalize(i128::from(minor), scale);
        Decimal { coefficient, scale }
    }

    pub fn parse_number(lexeme: &str) -> Result<Decimal, DecimalError> {
        scan(lexeme, true)
    }

    pub fn parse_text(text: &str) -> Result<Decimal, DecimalError> {
        scan(text.trim(), false)
    }

    pub fn from_json(value: &Value) -> Result<Decimal, DecimalError> {
        match value {
            Value::Number(number) => Decimal::parse_number(&number.to_string()),
            Value::String(text) => Decimal::parse_text(text),
            _ => Err(DecimalError::Invalid),
        }
    }

    pub fn is_zero(&self) -> bool {
        self.coefficient == 0
    }

    pub fn is_negative(&self) -> bool {
        self.coefficient < 0
    }

    pub fn is_positive(&self) -> bool {
        self.coefficient > 0
    }

    pub fn to_i64(&self) -> Option<i64> {
        (self.scale == 0)
            .then(|| i64::try_from(self.coefficient).ok())
            .flatten()
    }

    #[must_use]
    pub fn clamped_at_zero(self) -> Decimal {
        if self.is_negative() {
            Decimal::ZERO
        } else {
            self
        }
    }

    pub fn checked_add(&self, other: &Decimal) -> Option<Decimal> {
        self.combined(other, false)
    }

    pub fn checked_sub(&self, other: &Decimal) -> Option<Decimal> {
        self.combined(other, true)
    }

    pub fn ratio(&self, whole: &Decimal) -> Option<f64> {
        if whole.is_zero() {
            return None;
        }
        let magnitude = match self.aligned(whole) {
            Some((left, right, _)) => left as f64 / right as f64,
            None => self.to_f64().abs() / whole.to_f64().abs(),
        };
        Some(if self.is_negative() == whole.is_negative() {
            magnitude
        } else {
            -magnitude
        })
    }

    pub fn to_f64(&self) -> f64 {
        format!("{}e-{}", self.coefficient, self.scale)
            .parse()
            .unwrap_or(f64::NAN)
    }

    pub fn fixed(&self, places: u32) -> String {
        let magnitude = self.coefficient.unsigned_abs();
        let sign = if self.is_negative() { "-" } else { "" };
        let (whole, fraction) = if self.scale == 0 {
            (magnitude, String::new())
        } else {
            let divisor = 10_u128.pow(self.scale);
            (
                magnitude / divisor,
                format!(
                    "{:0width$}",
                    magnitude % divisor,
                    width = self.scale as usize
                ),
            )
        };
        let padding = (places.min(MAX_SCALE) as usize).saturating_sub(fraction.len());
        if fraction.is_empty() && padding == 0 {
            return format!("{sign}{whole}");
        }
        format!("{sign}{whole}.{fraction}{}", "0".repeat(padding))
    }

    fn aligned(&self, other: &Decimal) -> Option<(u128, u128, u32)> {
        let scale = self.scale.max(other.scale);
        let left = self
            .coefficient
            .unsigned_abs()
            .checked_mul(10_u128.checked_pow(scale - self.scale)?)?;
        let right = other
            .coefficient
            .unsigned_abs()
            .checked_mul(10_u128.checked_pow(scale - other.scale)?)?;
        Some((left, right, scale))
    }

    fn combined(&self, other: &Decimal, subtract: bool) -> Option<Decimal> {
        let (left, right, scale) = self.aligned(other)?;
        let right_negative = other.is_negative() != subtract;
        let (magnitude, negative) = if self.is_negative() == right_negative {
            (left.checked_add(right)?, self.is_negative())
        } else if left >= right {
            (left - right, self.is_negative())
        } else {
            (right - left, right_negative)
        };
        let coefficient = i128::try_from(magnitude).ok()?;
        Decimal::from_parts(if negative { -coefficient } else { coefficient }, scale)
    }

    fn magnitude_cmp(&self, other: &Decimal) -> Ordering {
        let split = |value: &Decimal| {
            let magnitude = value.coefficient.unsigned_abs();
            let divisor = 10_u128.pow(value.scale);
            (magnitude / divisor, magnitude % divisor)
        };
        let (left_whole, left_fraction) = split(self);
        let (right_whole, right_fraction) = split(other);
        let scale = self.scale.max(other.scale);
        left_whole.cmp(&right_whole).then_with(|| {
            let widen = |fraction: u128, own: u32| fraction * 10_u128.pow(scale - own);
            widen(left_fraction, self.scale).cmp(&widen(right_fraction, other.scale))
        })
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Decimal) -> Ordering {
        match (self.is_negative(), other.is_negative()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.magnitude_cmp(other),
            (true, true) => other.magnitude_cmp(self),
        }
    }
}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Decimal) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.fixed(0))
    }
}

fn normalize(mut coefficient: i128, mut scale: u32) -> (i128, u32) {
    if coefficient == 0 {
        return (0, 0);
    }
    while scale > 0 && coefficient % 10 == 0 {
        coefficient /= 10;
        scale -= 1;
    }
    (coefficient, scale)
}

fn digits(bytes: &[u8], from: usize) -> usize {
    bytes[from..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count()
}

fn scan(text: &str, strict: bool) -> Result<Decimal, DecimalError> {
    let bytes = text.as_bytes();
    let mut at = 0;
    let negative = bytes.first() == Some(&b'-');
    if negative || (!strict && bytes.first() == Some(&b'+')) {
        at += 1;
    }
    let whole = at..at + digits(bytes, at);
    at = whole.end;
    let mut fraction = at..at;
    let dotted = bytes.get(at) == Some(&b'.');
    if dotted {
        at += 1;
        fraction = at..at + digits(bytes, at);
        at = fraction.end;
    }
    let leading_zero = whole.len() > 1 && bytes[whole.start] == b'0';
    let shaped = if strict {
        !whole.is_empty() && !leading_zero && (!dotted || !fraction.is_empty())
    } else {
        !whole.is_empty() || !fraction.is_empty()
    };
    if !shaped {
        return Err(DecimalError::Invalid);
    }
    let mut exponent: i128 = 0;
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        let exponent_negative = bytes.get(at) == Some(&b'-');
        if exponent_negative || bytes.get(at) == Some(&b'+') {
            at += 1;
        }
        let run = digits(bytes, at);
        if run == 0 {
            return Err(DecimalError::Invalid);
        }
        for byte in &bytes[at..at + run] {
            exponent = (exponent * 10 + i128::from(byte - b'0')).min(EXPONENT_CEILING);
        }
        at += run;
        if exponent_negative {
            exponent = -exponent;
        }
    }
    if at != bytes.len() {
        return Err(DecimalError::Invalid);
    }
    assemble(
        negative,
        &bytes[whole.clone()],
        &bytes[fraction.clone()],
        exponent,
    )
}

fn assemble(
    negative: bool,
    whole: &[u8],
    fraction: &[u8],
    exponent: i128,
) -> Result<Decimal, DecimalError> {
    let mut run = whole
        .iter()
        .chain(fraction)
        .copied()
        .skip_while(|byte| *byte == b'0');
    let Some(first) = run.next() else {
        return Ok(Decimal::ZERO);
    };
    let mut significant = vec![first];
    significant.extend(run);
    let trailing = significant
        .iter()
        .rev()
        .take_while(|byte| **byte == b'0')
        .count();
    significant.truncate(significant.len() - trailing);
    if significant.len() > MAX_DIGITS {
        return Err(DecimalError::OutOfRange);
    }
    let power = exponent - fraction.len() as i128 + trailing as i128;
    let mut coefficient = significant
        .iter()
        .fold(0_i128, |sum, byte| sum * 10 + i128::from(byte - b'0'));
    let scale = if power >= 0 {
        let shift = u32::try_from(power).map_err(|_| DecimalError::OutOfRange)?;
        if significant.len() as u64 + u64::from(shift) > MAX_DIGITS as u64 {
            return Err(DecimalError::OutOfRange);
        }
        coefficient *= 10_i128.pow(shift);
        0
    } else {
        u32::try_from(-power)
            .ok()
            .filter(|scale| *scale <= MAX_SCALE)
            .ok_or(DecimalError::OutOfRange)?
    };
    if negative {
        coefficient = -coefficient;
    }
    Decimal::from_parts(coefficient, scale).ok_or(DecimalError::OutOfRange)
}
