// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod details;
pub use details::details_account;

use crate::accounts::ConnectedAccount;
use jackin_usage_provider_core::Decimal;
use serde_json::{Map, Value, json};

pub(crate) fn amount(value: &Value) -> Option<Value> {
    match value {
        Value::Number(_) => Some(value.clone()),
        Value::String(text) => Decimal::parse_text(text)
            .ok()
            .map(|exact| Value::String(exact.to_string())),
        _ => None,
    }
}

fn float(value: &Value) -> Option<f64> {
    match value {
        Value::String(text) => Decimal::parse_text(text).ok().map(|exact| exact.to_f64()),
        other => other.as_f64(),
    }
}

pub fn snapshot_account(
    account: &ConnectedAccount,
    fetched_at: Option<i64>,
    snapshot: Option<&Value>,
    now: i64,
) -> Value {
    let mut row = json!({
        "account":{"id":account.id,"provider":account.provider,"label":account.label},
        "stale":true,
        "usage":snapshot.map(public_usage)
    });
    if let Some(at) = fetched_at {
        let expires = at.saturating_add(600);
        row["fetched_at"] = at.into();
        row["expires_at"] = expires.into();
        row["stale"] = (now >= expires).into();
    }
    row
}

fn public_usage(snapshot: &Value) -> Value {
    let mut resources = Map::new();
    for (key, resource) in snapshot["resources"].as_object().into_iter().flatten() {
        if let Some(reading) = public_resource(resource) {
            resources.insert(key.clone(), reading);
        }
    }
    if let Some(resets) = resources.get_mut("rateLimitResets") {
        let mut expiries: Vec<_> = snapshot["resets"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|grant| grant["expiresAt"].as_i64())
            .collect();
        expiries.sort_unstable();
        if !expiries.is_empty() {
            resets["expires_at"] = json!(expiries);
        }
    }
    let mut usage = json!({"resources":resources});
    if let Some(plan) = snapshot["plan"].as_str() {
        usage["plan"] = plan.into();
    }
    usage
}

fn public_resource(resource: &Value) -> Option<Value> {
    let kind = resource["kind"].as_str()?;
    if !matches!(kind, "consumption" | "balance" | "amount" | "quota") {
        return None;
    }
    let unit = resource["unit"].as_str()?;
    let mut reading = json!({"kind":kind,"unit":unit});
    for field in ["used", "limit", "remaining", "available", "amount"] {
        if let Some(value) = amount(&resource[field]) {
            reading[field] = value;
        }
    }
    for field in [
        "remaining_fraction",
        "remaining_percent",
        "utilization",
        "starts_at",
        "exponent",
        "boost_permille",
        "base_remaining_percent",
        "remaining_ms",
        "usd",
    ] {
        if resource[field].is_number() {
            reading[field] = resource[field].clone();
        }
    }
    for (field, alias) in [
        ("resets_at", "resetsAt"),
        ("window_seconds", "windowSeconds"),
    ] {
        if let Some(n) = resource[field]
            .as_i64()
            .or_else(|| resource[alias].as_i64())
        {
            reading[field] = n.into();
        }
    }
    if reading.get("utilization").is_none()
        && let Some(limit) = float(&reading["limit"]).filter(|n| *n > 0.0)
        && let Some(used) = float(&reading["used"])
    {
        reading["utilization"] = json!(used / limit);
    }
    if let Some(currency) = resource["currency"]
        .as_str()
        .filter(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_alphabetic()))
    {
        reading["currency"] = currency.into();
    }
    if let Some(status @ ("not_in_plan" | "unlimited" | "exhausted" | "limited" | "unreported")) =
        resource["status"].as_str()
    {
        reading["status"] = status.into();
    }
    Some(reading)
}
