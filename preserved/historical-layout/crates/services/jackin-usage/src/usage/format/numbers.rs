// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Scalar, `JSON`, and amount helpers.

use chrono::{DateTime, Utc};

pub(crate) const PROCESS_OUTPUT_MAX: usize = 1024 * 1024;

pub(crate) fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Clamp a raw provider utilization into a `0..=100` "used" percentage.
///
/// Accepts both fraction form (`0.0..=1.0`) and already-percent form (`>1.0`).
/// Returns `None` for non-finite or negative inputs: several providers use a
/// negative sentinel (e.g. `-1`) for "unknown/unlimited", which must be omitted,
/// never fabricated into a full meter (`remaining_from_fraction(-0.5)` would
/// otherwise yield `Some(100)` — a "100% left" row for data that is absent).
pub(crate) fn used_percent_from_fraction(value: f64) -> Option<u8> {
    // The clamped-to-100 sibling of `used_percent_uncapped`: same fraction/percent
    // heuristic and absent-value guard, capped at 100 for the `% left` meter.
    used_percent_uncapped(value).map(|used| used.min(100) as u8)
}

pub(crate) fn remaining_from_fraction(value: f64) -> Option<u8> {
    used_percent_from_fraction(value).map(|used| 100u8.saturating_sub(used))
}

pub(crate) fn used_percent_label(value: f64) -> Option<String> {
    // Surface over-cap usage truthfully: a window the API reports above its limit
    // renders e.g. `150% used` rather than being clamped to `100% used` (Bug 11 —
    // the clamp silently discarded the overage the API provided). `remaining`
    // stays clamped at 0 (nothing left / bar full); only the used side carries the
    // overage.
    used_percent_uncapped(value).map(|used| format!("{used}% used"))
}

/// Used-percent without the upper clamp `used_percent_from_fraction` applies, so
/// an over-cap window keeps its true figure (e.g. `150`). Treats a value `<= 1.0`
/// as a fraction (×100) and a larger value as an already-scaled percent, matching
/// the fraction/percent heuristic the rest of the module uses.
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

pub(crate) fn parse_iso_epoch(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Utc).timestamp())
}

pub(crate) fn json_number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

pub(crate) fn format_amount_with_unit(value: f64, unit: &str) -> String {
    let amount = if value.fract().abs() < f64::EPSILON {
        format!("{}", value as i64)
    } else {
        format!("{value:.2}")
    };
    format!("{amount} {unit}")
}
