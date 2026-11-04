// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{Bucket, json_number, json_text, parse_iso_epoch};
use serde_json::Value;

const SESSION_SECONDS: i64 = 18_000;
const WEEK_SECONDS: i64 = 604_800;
const ROUTINES_KEYS: [&str; 5] = [
    "seven_day_routines",
    "seven_day_claude_routines",
    "claude_routines",
    "routines",
    "seven_day_cowork",
];

pub(crate) fn buckets(body: &Value) -> Vec<Bucket> {
    let mut out: Vec<Bucket> = body
        .get("limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(from_limit)
        .collect();
    for window in legacy(body) {
        let duplicate = out
            .iter()
            .any(|have| have.label.eq_ignore_ascii_case(&window.label));
        if !duplicate {
            out.push(window);
        }
    }
    out
}

fn reset(value: Option<&Value>) -> Option<i64> {
    value.and_then(json_text).and_then(parse_iso_epoch)
}

fn from_limit(limit: &Value) -> Option<Bucket> {
    let used = limit.get("percent").and_then(json_number)?;
    let (prefix, seconds) = match limit.get("kind").and_then(json_text)? {
        "session" => (None, SESSION_SECONDS),
        "weekly_all" => (None, WEEK_SECONDS),
        "weekly_scoped" => {
            let name = limit
                .pointer("/scope/model/display_name")
                .and_then(json_text)?;
            (Some(name), WEEK_SECONDS)
        }
        _ => return None,
    };
    Some(
        Bucket::timed(prefix, Some(seconds), None, "Claude")
            .used(Some(used))
            .resets(reset(limit.get("resets_at"))),
    )
}

fn legacy(body: &Value) -> Vec<Bucket> {
    let routines = ROUTINES_KEYS
        .iter()
        .find_map(|key| body.get(key).filter(|value| value.is_object()));
    let keys = [
        (body.get("five_hour"), None, SESSION_SECONDS),
        (body.get("seven_day"), None, WEEK_SECONDS),
        (body.get("seven_day_sonnet"), Some("Sonnet"), WEEK_SECONDS),
        (body.get("seven_day_opus"), Some("Opus"), WEEK_SECONDS),
        (routines, Some("Routines"), WEEK_SECONDS),
    ];
    keys.into_iter()
        .filter_map(|(window, prefix, seconds)| {
            let window = window.filter(|value| value.is_object())?;
            Some(
                Bucket::timed(prefix, Some(seconds), None, "Claude")
                    .used(window.get("utilization").and_then(json_number))
                    .resets(reset(window.get("resets_at"))),
            )
        })
        .collect()
}
