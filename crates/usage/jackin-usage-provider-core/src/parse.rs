// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::model::Money;

const MILLISECOND_EPOCH_FLOOR: f64 = 1_000_000_000_000.0;

pub fn json_number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

pub fn json_integer(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

pub fn json_text(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

pub fn parse_iso_epoch(text: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(text.trim())
        .ok()
        .map(|date| date.with_timezone(&Utc).timestamp())
}

pub fn epoch_seconds(value: f64) -> Option<i64> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let seconds = if value > MILLISECOND_EPOCH_FLOOR {
        value / 1000.0
    } else {
        value
    };
    Some(seconds.floor() as i64)
}

pub fn json_epoch(value: &Value) -> Option<i64> {
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if let Some(parsed) = parse_iso_epoch(trimmed) {
            return Some(parsed);
        }
        return trimmed.parse::<f64>().ok().and_then(epoch_seconds);
    }
    value.as_f64().and_then(epoch_seconds)
}

pub fn json_money(value: &Value, exponent: u32, unit: &str) -> Option<Money> {
    match value {
        Value::String(text) => Money::parse_decimal(text, exponent, unit),
        other => Money::from_f64(other.as_f64()?, exponent, unit),
    }
}

pub fn title_words(text: &str) -> String {
    text.split(|c: char| c == '_' || c == '-' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}
