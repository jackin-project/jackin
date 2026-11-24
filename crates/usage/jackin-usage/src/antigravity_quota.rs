// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{ProviderError, parse_iso_epoch};
use serde_json::{Map, Value, json};

const SESSION: i64 = 5 * 60 * 60;
const WEEK: i64 = 7 * 24 * 60 * 60;
const WINDOWS: [(&str, &str, i64); 4] = [
    ("gemini-5h", "Gemini 5h", SESSION),
    ("gemini-weekly", "Gemini weekly", WEEK),
    ("3p-5h", "Claude 5h", SESSION),
    ("3p-weekly", "Claude weekly", WEEK),
];
const ARGON_GROUP: &str = "Gemini 4 Argon";
const ARGON_ID: &str = "gemini-argon-wtu-weekly";
const PAID: [(&str, &str); 3] = [
    ("g1-pro-tier", "Google AI Pro"),
    ("g1-ultra-tier", "Google AI Ultra"),
    ("g1-ultra-lite-tier", "Google AI Ultra Lite"),
];

pub(crate) fn plan_name(body: &Value) -> Option<String> {
    if let Some(id) = text(body.pointer("/paidTier/id"))
        && let Some((_, name)) = PAID.iter().find(|(key, _)| *key == id)
    {
        return Some((*name).to_owned());
    }
    text(body.pointer("/paidTier/name"))
        .and_then(format_plan)
        .or_else(|| text(body.pointer("/currentTier/name")).and_then(format_plan))
}

pub(crate) fn project_id(body: &Value) -> Option<String> {
    text(body.get("cloudaicompanionProject"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn is_complete(body: &Value) -> bool {
    let Some(groups) = groups(body) else {
        return false;
    };
    let present: Vec<&str> = shared_buckets(&groups)
        .filter_map(|bucket| {
            finite_fraction(bucket)?;
            bucket_id(bucket)
        })
        .collect();
    WINDOWS
        .iter()
        .all(|(id, _, _)| present.iter().any(|found| found == id))
}

pub(crate) fn merge(bodies: &[Value]) -> Result<Map<String, Value>, ProviderError> {
    let lists: Vec<Vec<&Value>> = bodies.iter().filter_map(groups).collect();
    if lists.is_empty() {
        return Err(ProviderError::Decode(
            "Antigravity quota summary has no groups",
        ));
    }
    let mut pooled: Vec<(&str, f64, Option<i64>)> = Vec::new();
    for list in &lists {
        for bucket in shared_buckets(list) {
            let Some(id) = bucket_id(bucket).filter(|id| window_id(id).is_some()) else {
                continue;
            };
            if pooled.iter().any(|(found, _, _)| *found == id) {
                continue;
            }
            let Some(fraction) = finite_fraction(bucket) else {
                continue;
            };
            pooled.push((id, fraction, reset_of(bucket)));
        }
    }
    let argon = lists.iter().find_map(|list| argon_window(list));
    let mut resources = Map::new();
    for (id, label, period) in WINDOWS.iter().take(2) {
        insert_window(&mut resources, &pooled, id, label, *period);
    }
    if let Some(window) = argon {
        resources.insert(ARGON_ID.to_owned(), window);
    }
    for (id, label, period) in WINDOWS.iter().skip(2) {
        insert_window(&mut resources, &pooled, id, label, *period);
    }
    let saw_bucket = lists
        .iter()
        .any(|groups| groups.iter().any(|group| buckets(group).next().is_some()));
    if resources.is_empty() && saw_bucket {
        return Err(ProviderError::Decode(
            "Antigravity quota summary has no usable windows",
        ));
    }
    Ok(resources)
}

fn insert_window(
    resources: &mut Map<String, Value>,
    pooled: &[(&str, f64, Option<i64>)],
    id: &str,
    label: &str,
    period: i64,
) {
    if let Some((_, fraction, reset)) = pooled.iter().find(|(found, _, _)| *found == id) {
        resources.insert(id.to_owned(), window(label, period, *fraction, *reset));
    }
}

fn argon_window(groups: &[&Value]) -> Option<Value> {
    let argon: Vec<_> = groups
        .iter()
        .copied()
        .filter(|group| text(group.get("displayName")) == Some(ARGON_GROUP))
        .collect();
    if argon.is_empty() {
        return None;
    }
    let buckets: Vec<&Value> = argon.iter().flat_map(|group| buckets(group)).collect();
    let weekly: Vec<&Value> = buckets
        .iter()
        .copied()
        .filter(|bucket| text(bucket.get("window")) == Some("weekly"))
        .collect();
    if weekly.len() != 1 {
        return None;
    }
    let bucket = weekly[0];
    let id = bucket_id(bucket).filter(|id| !id.is_empty() && window_id(id).is_none())?;
    let _id = id;
    if disabled(bucket) != Disabled::Open {
        return None;
    }
    let fraction = finite_fraction(bucket)?;
    Some(window("Argon weekly", WEEK, fraction, reset_of(bucket)))
}

fn window(label: &str, period: i64, fraction: f64, reset: Option<i64>) -> Value {
    let mut resource = json!({
        "label": label,
        "kind": "quota",
        "unit": "percent",
        "remaining_fraction": fraction.clamp(0.0, 1.0),
        "window_seconds": period,
    });
    if let Some(reset) = reset {
        resource["resets_at"] = reset.into();
    }
    resource
}

fn groups(body: &Value) -> Option<Vec<&Value>> {
    let root = body
        .get("response")
        .filter(|value| value.is_object())
        .unwrap_or(body);
    Some(root.get("groups")?.as_array()?.iter().collect())
}

fn shared_buckets<'a>(groups: &'a [&'a Value]) -> impl Iterator<Item = &'a Value> {
    groups
        .iter()
        .filter_map(|group| (text(group.get("displayName")) != Some(ARGON_GROUP)).then_some(*group))
        .flat_map(buckets)
}

fn buckets(group: &Value) -> impl Iterator<Item = &Value> {
    group
        .get("buckets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn bucket_id(bucket: &Value) -> Option<&str> {
    text(bucket.get("bucketId"))
}

fn window_id(id: &str) -> Option<&'static str> {
    WINDOWS
        .iter()
        .find(|(key, _, _)| *key == id)
        .map(|(key, _, _)| *key)
}

fn finite_fraction(bucket: &Value) -> Option<f64> {
    bucket
        .get("remainingFraction")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

fn reset_of(bucket: &Value) -> Option<i64> {
    text(bucket.get("resetTime")).and_then(parse_iso_epoch)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Disabled {
    Open,
    Closed,
}

fn disabled(bucket: &Value) -> Disabled {
    match bucket.get("disabled") {
        None | Some(Value::Null) => Disabled::Open,
        Some(Value::Bool(false)) => Disabled::Open,
        Some(Value::Bool(true)) => Disabled::Closed,
        Some(_) => Disabled::Closed,
    }
}

fn text(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str)
}

fn format_plan(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    for (needle, label) in [
        ("ultra lite", "Google AI Ultra Lite"),
        ("ultra", "Google AI Ultra"),
        ("pro", "Google AI Pro"),
    ] {
        if lower.contains(needle) {
            return Some(label.to_owned());
        }
    }
    Some(title_case(trimmed))
}

fn title_case(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let mut out = String::new();
                    out.extend(first.to_uppercase());
                    out.push_str(chars.as_str());
                    out
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
