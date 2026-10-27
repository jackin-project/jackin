use std::cmp::Ordering;

use super::Money;

pub(super) fn raw_percent_of(used: &Money, cap: &Money) -> Option<i32> {
    if used.currency != cap.currency || cap.amount_minor <= 0 {
        return None;
    }
    if used.amount_minor == 0 {
        return Some(0);
    }

    let negative = used.amount_minor.is_negative();
    let mut numerator = u128::from(used.amount_minor.unsigned_abs()) * 100;
    let mut denominator = u128::try_from(cap.amount_minor).ok()?;
    let exponent_delta = i16::from(cap.exponent) - i16::from(used.exponent);
    if exponent_delta >= 0 {
        let limit = if negative {
            u128::from(i32::MIN.unsigned_abs())
        } else {
            u128::from(i32::MAX as u32)
        };
        let saturation_threshold = denominator.checked_mul(limit + 1)?;
        for _ in 0..exponent_delta {
            if numerator >= saturation_threshold {
                return Some(if negative { i32::MIN } else { i32::MAX });
            }
            numerator = numerator.checked_mul(10)?;
        }
    } else {
        for _ in exponent_delta..0 {
            if denominator > numerator {
                return Some(0);
            }
            denominator = denominator.checked_mul(10)?;
        }
    }

    let quotient = numerator / denominator;
    let limit = if negative {
        u128::from(i32::MIN.unsigned_abs())
    } else {
        u128::from(i32::MAX as u32)
    };
    if quotient > limit {
        return Some(if negative { i32::MIN } else { i32::MAX });
    }
    if negative {
        if quotient == u128::from(i32::MIN.unsigned_abs()) {
            Some(i32::MIN)
        } else {
            Some(-i32::try_from(quotient).ok()?)
        }
    } else {
        i32::try_from(quotient).ok()
    }
}

pub(super) fn raw_percent_points_of(used: &Money, cap: &Money) -> Option<f64> {
    if used.currency != cap.currency || cap.amount_minor <= 0 {
        return None;
    }

    let exponent_delta = i32::from(cap.exponent) - i32::from(used.exponent);
    let points =
        (used.amount_minor as f64 / cap.amount_minor as f64) * 10_f64.powi(exponent_delta) * 100.0;
    points.is_finite().then_some(points)
}

pub(super) fn checked_cmp(left: &Money, right: &Money) -> Option<Ordering> {
    if left.currency != right.currency {
        return None;
    }

    let exponent = left.exponent.max(right.exponent);
    Some(compare_signed(
        left.amount_minor,
        right.amount_minor,
        exponent - left.exponent,
        exponent - right.exponent,
    ))
}

pub(super) fn checked_difference(left: &Money, right: &Money) -> Option<Money> {
    if left.currency != right.currency {
        return None;
    }

    let exponent = left.exponent.max(right.exponent);
    let left_digits = scaled_digits(left.amount_minor, exponent - left.exponent);
    let right_digits = scaled_digits(right.amount_minor, exponent - right.exponent);
    let left_sign = sign(left.amount_minor);
    let right_sign = -sign(right.amount_minor);
    let (result_sign, result_digits) =
        signed_sum(left_sign, left_digits, right_sign, right_digits)?;
    digits_to_money(result_sign, result_digits, exponent, left.currency.clone())
}

pub(super) fn checked_nonnegative_difference(left: &Money, right: &Money) -> Option<Money> {
    let exponent = left.exponent.max(right.exponent);
    if checked_cmp(left, right)? == Ordering::Less {
        return Some(Money::new(0, left.currency.clone(), exponent));
    }
    checked_difference(left, right)
}

fn compare_signed(left: i64, right: i64, left_shift: u8, right_shift: u8) -> Ordering {
    let left_sign = sign(left);
    let right_sign = sign(right);
    if left_sign != right_sign {
        return left_sign.cmp(&right_sign);
    }
    if left_sign == 0 {
        return Ordering::Equal;
    }

    let left_digits = scaled_digits(left, left_shift);
    let right_digits = scaled_digits(right, right_shift);
    let magnitude_order = compare_digits(&left_digits, &right_digits);
    if left_sign < 0 {
        magnitude_order.reverse()
    } else {
        magnitude_order
    }
}

fn signed_sum(
    left_sign: i8,
    left_digits: Vec<u8>,
    right_sign: i8,
    right_digits: Vec<u8>,
) -> Option<(i8, Vec<u8>)> {
    if left_sign == 0 {
        return Some((right_sign, right_digits));
    }
    if right_sign == 0 {
        return Some((left_sign, left_digits));
    }
    if left_sign == right_sign {
        return Some((left_sign, add_digits(&left_digits, &right_digits)));
    }

    match compare_digits(&left_digits, &right_digits) {
        Ordering::Greater => Some((left_sign, subtract_digits(&left_digits, &right_digits)?)),
        Ordering::Less => Some((right_sign, subtract_digits(&right_digits, &left_digits)?)),
        Ordering::Equal => Some((0, vec![0])),
    }
}

fn scaled_digits(amount: i64, shift: u8) -> Vec<u8> {
    let mut digits = amount
        .unsigned_abs()
        .to_string()
        .bytes()
        .rev()
        .map(|digit| digit - b'0')
        .collect::<Vec<_>>();
    for _ in 0..shift {
        digits.insert(0, 0);
    }
    normalize_digits(&mut digits);
    digits
}

fn compare_digits(left: &[u8], right: &[u8]) -> Ordering {
    match left.len().cmp(&right.len()) {
        Ordering::Equal => left.iter().rev().cmp(right.iter().rev()),
        order => order,
    }
}

fn add_digits(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0;
    for index in 0..left.len().max(right.len()) {
        let sum = u16::from(*left.get(index).unwrap_or(&0))
            + u16::from(*right.get(index).unwrap_or(&0))
            + carry;
        result.push((sum % 10) as u8);
        carry = sum / 10;
    }
    if carry > 0 {
        result.push(carry as u8);
    }
    normalize_digits(&mut result);
    result
}

fn subtract_digits(larger: &[u8], smaller: &[u8]) -> Option<Vec<u8>> {
    let mut result = Vec::with_capacity(larger.len());
    let mut borrow = 0_u16;
    for (index, digit) in larger.iter().enumerate() {
        let minuend = u16::from(*digit);
        let subtrahend = u16::from(*smaller.get(index).unwrap_or(&0)) + borrow;
        if minuend < subtrahend {
            result.push(u8::try_from(10 + minuend - subtrahend).ok()?);
            borrow = 1;
        } else {
            result.push(u8::try_from(minuend - subtrahend).ok()?);
            borrow = 0;
        }
    }
    if borrow != 0 {
        return None;
    }
    normalize_digits(&mut result);
    Some(result)
}

fn digits_to_money(
    sign: i8,
    mut digits: Vec<u8>,
    mut exponent: u8,
    currency: String,
) -> Option<Money> {
    normalize_digits(&mut digits);
    if digits.len() == 1 && digits.first() == Some(&0) {
        return Some(Money::new(0, currency, exponent));
    }

    let maximum = if sign < 0 {
        i64::MAX as u64 + 1
    } else {
        i64::MAX as u64
    };
    loop {
        if let Some(magnitude) = digits_to_u64(&digits)
            && magnitude <= maximum
        {
            return Some(Money::new(
                signed_magnitude(sign, magnitude)?,
                currency,
                exponent,
            ));
        }

        if exponent == 0 || digits.first() != Some(&0) {
            return None;
        }
        digits.remove(0);
        exponent -= 1;
        normalize_digits(&mut digits);
    }
}

fn signed_magnitude(sign: i8, magnitude: u64) -> Option<i64> {
    match (sign, magnitude) {
        (-1, magnitude) if magnitude == i64::MAX as u64 + 1 => Some(i64::MIN),
        (-1, magnitude) => i64::try_from(magnitude).ok()?.checked_neg(),
        (0, 0) => Some(0),
        (1, magnitude) => i64::try_from(magnitude).ok(),
        _ => None,
    }
}

fn digits_to_u64(digits: &[u8]) -> Option<u64> {
    let decimal = digits
        .iter()
        .rev()
        .map(|digit| char::from(b'0' + *digit))
        .collect::<String>();
    decimal.parse().ok()
}

fn normalize_digits(digits: &mut Vec<u8>) {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
}

fn sign(value: i64) -> i8 {
    match value.cmp(&0) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}
