// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use serde_json::Value;

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
