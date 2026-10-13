// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Text parsing, currency, and path helpers.

use std::path::{Path, PathBuf};

pub(crate) fn dollar_amounts(text: &str) -> Vec<f64> {
    let mut values = Vec::new();
    let mut rest = text;
    while let Some(index) = rest.find('$') {
        rest = &rest[index + 1..];
        let amount: String = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit() || matches!(ch, '.' | ','))
            .filter(|ch| *ch != ',')
            .collect();
        if let Ok(value) = amount.parse() {
            values.push(value);
        }
    }
    values
}

pub(crate) fn percent_before_used(text: &str) -> Option<f64> {
    let before_used = text.split("% used").next()?;
    let percent = before_used
        .rsplit(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .find(|part| !part.is_empty())?;
    percent.parse().ok()
}

pub(crate) fn format_currency(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("${value:.0}")
    } else {
        format!("${value:.2}")
    }
}

pub(crate) fn format_cents(value: i64) -> String {
    format_currency(value as f64 / 100.0)
}

pub(crate) fn codex_account_from_value(value: &serde_json::Value) -> Option<String> {
    value
        .pointer("/tokens/email")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            value
                .pointer("/tokens/account_id")
                .and_then(serde_json::Value::as_str)
        })
        .or_else(|| value.get("auth_mode").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
}
pub(crate) fn first_string_key(value: &serde_json::Value, needle: &str) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(found) = map.get(needle).and_then(serde_json::Value::as_str) {
                return Some(found.to_owned());
            }
            map.values().find_map(|v| first_string_key(v, needle))
        }
        serde_json::Value::Array(values) => values.iter().find_map(|v| first_string_key(v, needle)),
        _ => None,
    }
}

pub(crate) fn home_path(rel: &str) -> PathBuf {
    let rel = rel.trim_start_matches('/');
    std::env::var("HOME")
        .map_or_else(|_| PathBuf::from("/home/agent"), PathBuf::from)
        .join(rel)
}
pub(crate) fn oauth_origin(path: &Path) -> String {
    // `to_string_lossy` borrows (no alloc) for the common UTF-8 path and only
    // allocates for non-UTF-8 container paths; `&Cow<str>` coerces to `&str`.
    format!(
        "OAuth · {}",
        jackin_core::shorten_home(&path.to_string_lossy())
    )
}
pub(crate) fn titlecase_ascii(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::new();
    out.extend(first.to_uppercase());
    out.push_str(chars.as_str());
    out
}
pub(crate) fn compact_count(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.1}B", value as f64 / 1_000_000_000.0)
    } else if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}
