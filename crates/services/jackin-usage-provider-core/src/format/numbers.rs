// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};

pub const PROCESS_OUTPUT_MAX: usize = 1024 * 1024;

pub fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

pub fn used_percent_from_fraction(value: f64) -> Option<u8> {
    used_percent_uncapped(value).map(|used| used.min(100) as u8)
}

pub fn remaining_from_fraction(value: f64) -> Option<u8> {
    used_percent_from_fraction(value).map(|used| 100u8.saturating_sub(used))
}

pub fn used_percent_label(value: f64) -> Option<String> {
    used_percent_uncapped(value).map(|used| format!("{used}% used"))
}

pub(crate) fn used_percent_uncapped(value: f64) -> Option<u16> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let used = if value <= 1.0 { value * 100.0 } else { value };
    #[expect(
        clippy::cast_sign_loss,
        reason = "value filtered non-negative above; clamp bounds the f64→u16 cast"
    )]
    {
        Some(used.round().clamp(0.0, f64::from(u16::MAX)) as u16)
    }
}

pub fn parse_iso_epoch(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Utc).timestamp())
}

pub fn json_number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

pub fn json_epoch_seconds(value: &serde_json::Value) -> Option<i64> {
    let number = json_number(value)?;
    if number > 1_000_000_000_000.0 {
        Some((number / 1000.0).floor() as i64)
    } else {
        Some(number.floor() as i64)
    }
}

pub fn format_amount_with_unit(value: f64, unit: &str) -> String {
    let amount = if value.fract().abs() < f64::EPSILON {
        format!("{}", value as i64)
    } else {
        format!("{value:.2}")
    };
    format!("{amount} {unit}")
}
