// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::Money;

const MAX_DIGITS: usize = 288;
const MAX_SCALE: usize = u8::MAX as usize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Decimal {
    negative: bool,
    digits: [u8; MAX_DIGITS],
    len: usize,
    scale: u8,
}

impl Decimal {
    fn parse(raw: &str) -> Option<Self> {
        let bytes = raw.as_bytes();
        let mut index = 0;
        let negative = match bytes.first()? {
            b'-' => {
                index += 1;
                true
            }
            b'+' => return None,
            _ => false,
        };
        let mut big_endian = [0; MAX_DIGITS];
        let mut len = 0;
        let mut pending_zeros = 0usize;
        let mut fractional_digits = 0i64;
        let mut after_decimal = false;
        let mut saw_digit = false;
        while let Some(byte) = bytes.get(index).copied() {
            match byte {
                b'0'..=b'9' => {
                    saw_digit = true;
                    if after_decimal {
                        fractional_digits = fractional_digits.checked_add(1)?;
                    }
                    let digit = byte - b'0';
                    if len == 0 && digit == 0 {
                        index += 1;
                        continue;
                    }
                    if digit == 0 {
                        pending_zeros = pending_zeros.checked_add(1)?;
                    } else {
                        flush_pending_zeros(&mut big_endian, &mut len, &mut pending_zeros)?;
                        push_digit(&mut big_endian, &mut len, digit)?;
                    }
                }
                b'.' if !after_decimal => after_decimal = true,
                b'e' | b'E' => break,
                _ => return None,
            }
            index += 1;
        }
        if !saw_digit {
            return None;
        }
        if len == 0 {
            return Some(Self {
                negative: false,
                digits: [0; MAX_DIGITS],
                len: 1,
                scale: 0,
            });
        }
        let exponent = if bytes
            .get(index)
            .is_some_and(|byte| matches!(byte, b'e' | b'E'))
        {
            parse_exponent(bytes, index + 1)?
        } else {
            if index != bytes.len() {
                return None;
            }
            0
        };
        let pending_zeros = i64::try_from(pending_zeros).ok()?;
        let mut scale = fractional_digits
            .checked_sub(exponent)?
            .checked_sub(pending_zeros)?;
        if scale < 0 {
            let append = usize::try_from(scale.checked_neg()?).ok()?;
            for _ in 0..append {
                push_digit(&mut big_endian, &mut len, 0)?;
            }
            scale = 0;
        }
        if usize::try_from(scale).ok()? > MAX_SCALE {
            return None;
        }
        let mut digits = [0; MAX_DIGITS];
        for (target, source) in digits.iter_mut().zip(big_endian[..len].iter().rev()) {
            *target = *source;
        }
        Some(Self {
            negative,
            digits,
            len,
            scale: u8::try_from(scale).ok()?,
        })
    }

    pub(crate) fn from_value(value: &serde_json::Value) -> Option<Self> {
        value
            .as_number()
            .and_then(|number| Self::parse(number.as_str()))
    }

    pub(crate) const fn is_negative(&self) -> bool {
        self.negative
    }

    pub(crate) const fn scale(&self) -> u8 {
        self.scale
    }

    pub(crate) fn money_at_scale(&self, scale: u8) -> Option<Money> {
        if scale < self.scale {
            return None;
        }
        let shift = usize::from(scale - self.scale);
        let len = self.len.checked_add(shift)?;
        if len > MAX_DIGITS {
            return None;
        }
        let mut coefficient = 0u128;
        for index in (0..self.len).rev() {
            coefficient = coefficient
                .checked_mul(10)?
                .checked_add(u128::from(self.digits[index]))?;
        }
        for _ in 0..shift {
            coefficient = coefficient.checked_mul(10)?;
        }
        let limit = if self.negative {
            (i64::MAX as u128) + 1
        } else {
            i64::MAX as u128
        };
        if coefficient > limit {
            return None;
        }
        let amount_minor = if self.negative {
            if coefficient == (i64::MAX as u128) + 1 {
                i64::MIN
            } else {
                -i64::try_from(coefficient).ok()?
            }
        } else {
            i64::try_from(coefficient).ok()?
        };
        Some(Money::new(amount_minor, "USD", scale))
    }

    pub(crate) fn to_money(&self) -> Option<Money> {
        let preferred_scale = self.scale.max(2);
        self.money_at_scale(preferred_scale)
            .or_else(|| self.money_at_scale(self.scale))
    }

    pub(crate) fn saturating_sub(&self, other: &Self) -> Option<Self> {
        if self.negative || other.negative {
            return None;
        }
        let scale = self.scale.max(other.scale);
        let mut left = self.aligned(scale)?;
        let right = other.aligned(scale)?;
        if compare_magnitude(&left, &right) == std::cmp::Ordering::Less {
            return Some(Self::zero(scale));
        }
        let mut borrow = 0i16;
        for index in 0..left.len {
            let right_digit = i16::from(right.digits.get(index).copied().unwrap_or(0));
            let mut digit = i16::from(left.digits[index]) - right_digit - borrow;
            if digit < 0 {
                digit += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }
            left.digits[index] = u8::try_from(digit).ok()?;
        }
        while left.len > 1 && left.digits[left.len - 1] == 0 {
            left.len -= 1;
        }
        left.negative = false;
        left.scale = scale;
        Some(left.normalized())
    }

    pub(crate) fn to_f64(&self) -> Option<f64> {
        let mut value = 0.0;
        for index in (0..self.len).rev() {
            value = value * 10.0 + f64::from(self.digits[index]);
        }
        let value = value / 10f64.powi(i32::from(self.scale));
        let value = if self.negative { -value } else { value };
        value.is_finite().then_some(value)
    }

    fn zero(scale: u8) -> Self {
        Self {
            negative: false,
            digits: [0; MAX_DIGITS],
            len: 1,
            scale,
        }
    }

    fn aligned(&self, scale: u8) -> Option<Self> {
        if scale < self.scale {
            return None;
        }
        let shift = usize::from(scale - self.scale);
        let len = self.len.checked_add(shift)?;
        if len > MAX_DIGITS {
            return None;
        }
        let mut digits = [0; MAX_DIGITS];
        digits[shift..len].copy_from_slice(&self.digits[..self.len]);
        Some(Self {
            negative: self.negative,
            digits,
            len,
            scale,
        })
    }

    fn normalized(mut self) -> Self {
        while self.scale > 0 && self.len > 1 && self.digits[0] == 0 {
            self.digits.copy_within(1..self.len, 0);
            self.len -= 1;
            self.scale -= 1;
        }
        self
    }
}

pub(crate) fn sum_nonnegative<'a>(
    values: impl IntoIterator<Item = &'a serde_json::Value>,
) -> Option<Money> {
    let mut accumulator = DecimalAccumulator::default();
    let mut found = false;
    for value in values {
        if value.as_number().is_none() {
            continue;
        }
        let decimal = Decimal::from_value(value)?;
        if decimal.is_negative() {
            continue;
        }
        accumulator.add(&decimal)?;
        found = true;
    }
    found.then(|| accumulator.into_money()).flatten()
}

pub(crate) fn format_money(money: &Money) -> String {
    let negative = money.amount_minor < 0;
    let digits = money.amount_minor.unsigned_abs().to_string();
    let scale = usize::from(money.exponent);
    if scale == 0 {
        return format!("${}{digits}", if negative { "-" } else { "" });
    }
    let (integer, mut fractional) = if digits.len() <= scale {
        let zeros = "0".repeat(scale - digits.len() + 1);
        let padded = format!("{zeros}{digits}");
        let split = padded.len() - scale;
        (padded[..split].to_owned(), padded[split..].to_owned())
    } else {
        let split = digits.len() - scale;
        (digits[..split].to_owned(), digits[split..].to_owned())
    };
    while fractional.ends_with('0') {
        fractional.pop();
    }
    if fractional.is_empty() {
        format!("${}{}", if negative { "-" } else { "" }, integer)
    } else {
        while fractional.len() < 2 {
            fractional.push('0');
        }
        format!(
            "${}{}.{}",
            if negative { "-" } else { "" },
            integer,
            fractional
        )
    }
}

struct DecimalAccumulator {
    digits: [u8; MAX_DIGITS],
    len: usize,
    scale: u8,
    found: bool,
}

impl Default for DecimalAccumulator {
    fn default() -> Self {
        Self {
            digits: [0; MAX_DIGITS],
            len: 0,
            scale: 0,
            found: false,
        }
    }
}

impl DecimalAccumulator {
    fn add(&mut self, value: &Decimal) -> Option<()> {
        if value.negative {
            return None;
        }
        if !self.found {
            self.digits[..value.len].copy_from_slice(&value.digits[..value.len]);
            self.len = value.len;
            self.scale = value.scale;
            self.found = true;
            return Some(());
        }
        let scale = self.scale.max(value.scale);
        let shift = usize::from(scale - self.scale);
        let value_shift = usize::from(scale - value.scale);
        if self.len.checked_add(shift)? > MAX_DIGITS
            || value.len.checked_add(value_shift)? > MAX_DIGITS
        {
            return None;
        }
        if shift > 0 {
            self.digits.copy_within(0..self.len, shift);
            self.digits[..shift].fill(0);
            self.len += shift;
        }
        self.scale = scale;
        let value_len = value.len + value_shift;
        let max_len = self.len.max(value_len);
        let mut carry = 0u8;
        for index in 0..max_len {
            let left_digit = self.digits.get(index).copied().unwrap_or(0);
            let right_digit = if index >= value_shift {
                value.digits.get(index - value_shift).copied().unwrap_or(0)
            } else {
                0
            };
            let sum = left_digit + right_digit + carry;
            self.digits[index] = sum % 10;
            carry = sum / 10;
        }
        self.len = max_len;
        if carry > 0 {
            if self.len == MAX_DIGITS {
                return None;
            }
            self.digits[self.len] = carry;
            self.len += 1;
        }
        while self.len > 1 && self.digits[self.len - 1] == 0 {
            self.len -= 1;
        }
        Some(())
    }

    fn into_money(mut self) -> Option<Money> {
        while self.scale > 0 && self.len > 1 && self.digits[0] == 0 {
            self.digits.copy_within(1..self.len, 0);
            self.len -= 1;
            self.scale -= 1;
        }
        let mut decimal = Decimal {
            negative: false,
            digits: self.digits,
            len: self.len,
            scale: self.scale,
        };
        decimal = decimal.normalized();
        decimal.to_money()
    }
}

fn compare_magnitude(left: &Decimal, right: &Decimal) -> std::cmp::Ordering {
    left.len.cmp(&right.len).then_with(|| {
        (0..left.len)
            .rev()
            .map(|index| left.digits[index])
            .cmp((0..right.len).rev().map(|index| right.digits[index]))
    })
}

fn push_digit(digits: &mut [u8; MAX_DIGITS], len: &mut usize, digit: u8) -> Option<()> {
    if *len == MAX_DIGITS {
        return None;
    }
    digits[*len] = digit;
    *len += 1;
    Some(())
}

fn flush_pending_zeros(
    digits: &mut [u8; MAX_DIGITS],
    len: &mut usize,
    pending_zeros: &mut usize,
) -> Option<()> {
    while *pending_zeros > 0 {
        push_digit(digits, len, 0)?;
        *pending_zeros -= 1;
    }
    Some(())
}

fn parse_exponent(bytes: &[u8], mut index: usize) -> Option<i64> {
    let negative = match bytes.get(index).copied()? {
        b'-' => {
            index += 1;
            true
        }
        b'+' => {
            index += 1;
            false
        }
        _ => false,
    };
    let mut exponent = 0i64;
    let mut found = false;
    while let Some(byte) = bytes.get(index).copied() {
        if !byte.is_ascii_digit() {
            return None;
        }
        found = true;
        exponent = exponent
            .checked_mul(10)?
            .checked_add(i64::from(byte - b'0'))?;
        index += 1;
    }
    if !found {
        return None;
    }
    if negative {
        exponent.checked_neg()
    } else {
        Some(exponent)
    }
}

#[cfg(test)]
mod tests;
