// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::snapshot_account;
use crate::accounts::{RefreshFailure, StoredSnapshot};
use crate::dashboard::DashboardSettings;
use jackin_core::{Timestamp, plain};
use serde_json::{Map, Value, json};

const COUNTERS: &[&str] = &[
    "tokens",
    "inputTokens",
    "cacheReadTokens",
    "outputTokens",
    "reasoningTokens",
    "turns",
    "credits",
    "usageValue",
    "spend",
    "byokSpend",
];

fn fields(data: &Value, names: &[&str], valid: fn(&Value) -> bool) -> Map<String, Value> {
    names
        .iter()
        .filter_map(|&name| Some((name.into(), data.get(name)?.clone())))
        .filter(|(_, v)| valid(v))
        .collect()
}

fn numbers(data: &Value) -> Value {
    Value::Object(fields(data, COUNTERS, Value::is_number))
}

fn range(data: &Value) -> Value {
    let mut result = fields(data, &["observedDays", "expectedDays"], Value::is_u64);
    if data["totals"].is_object() {
        result.insert("totals".into(), numbers(&data["totals"]));
    }
    let models = data
        .get("byModel")
        .or_else(|| data.get("by_model"))
        .and_then(Value::as_object);
    if let Some(models) = models {
        result.insert(
            "by_model".into(),
            Value::Object(
                models
                    .iter()
                    .filter(|(_, v)| v.is_object())
                    .map(|(name, v)| (name.clone(), numbers(v)))
                    .collect(),
            ),
        );
    }
    Value::Object(result)
}

fn date(key: &str) -> bool {
    time::Date::parse(
        key,
        time::macros::format_description!("[year]-[month]-[day]"),
    )
    .is_ok()
}

fn days(data: &Value) -> Value {
    Value::Object(
        data.as_object()
            .into_iter()
            .flatten()
            .filter(|(key, v)| date(key) && v.is_object())
            .map(|(key, v)| (key.clone(), range(v)))
            .collect(),
    )
}

fn history(source: &Value) -> Value {
    let mut out = Map::new();
    for report in ["counts", "credits"] {
        if source[report]["days"].is_object() {
            out.insert(report.into(), json!({"days":days(&source[report]["days"])}));
        }
    }
    if let Some(ranges) = source["ranges"].as_object() {
        out.insert(
            "ranges".into(),
            Value::Object(
                ranges
                    .iter()
                    .filter(|(id, _)| matches!(id.as_str(), "today" | "yesterday" | "last30"))
                    .map(|(id, v)| (id.clone(), range(v)))
                    .collect(),
            ),
        );
    }
    Value::Object(out)
}

fn activity(source: &Value) -> Value {
    let mut out = fields(source, &["expected_days", "observed_days"], Value::is_u64);
    if let Some(as_of) = source["as_of"].as_str().filter(|s| date(s)) {
        out.insert("as_of".into(), as_of.into());
    }
    if source["days"].is_object() {
        out.insert("days".into(), days(&source["days"]));
    }
    if source["yesterday"].is_object() {
        out.insert("yesterday".into(), range(&source["yesterday"]));
    }
    if source["prior30Days"].is_object() {
        out.insert("prior30Days".into(), numbers(&source["prior30Days"]));
    }
    Value::Object(out)
}

fn daily(source: &Value) -> Value {
    let mut out = Map::new();
    for name in ["bills", "consumption"] {
        let data = &source[name];
        if !data.is_object() {
            continue;
        }
        let mut branch = fields(data, &["start", "end"], Value::is_string);
        branch.extend(fields(data, &["unavailable"], Value::is_boolean));
        if let Some(rows) = data["rows"].as_array() {
            branch.insert(
                "rows".into(),
                Value::Array(
                    rows.iter()
                        .map(|row| {
                            let mut result = fields(
                                row,
                                &[
                                    "date",
                                    "name",
                                    "resource_type",
                                    "currency",
                                    "inference_type",
                                ],
                                Value::is_string,
                            );
                            result.extend(fields(
                                row,
                                &[
                                    "consumed_amount",
                                    "final_paid_amount",
                                    "request_count",
                                    "total_tokens",
                                    "audio_transcription_seconds",
                                    "input_cache_hit_amount",
                                    "input_cache_miss_amount",
                                    "output_amount",
                                    "input_cache_hit_tokens",
                                    "input_cache_miss_tokens",
                                    "output_tokens",
                                    "original_amount",
                                    "discount_amount",
                                    "gift_amount",
                                ],
                                Value::is_number,
                            ));
                            Value::Object(result)
                        })
                        .collect(),
                ),
            );
        }
        out.insert(name.into(), Value::Object(branch));
    }
    Value::Object(out)
}

fn enrich(usage: &mut Value, data: &Value) {
    let Some(root) = usage.as_object_mut() else {
        return;
    };
    root.extend(fields(
        data,
        &[
            "project",
            "region",
            "authMethod",
            "key_type",
            "notice",
            "label",
        ],
        Value::is_string,
    ));
    root.extend(fields(
        data,
        &[
            "is_available",
            "auto_renew",
            "claw_enabled",
            "balanceAlertsEnabled",
        ],
        Value::is_boolean,
    ));
    root.extend(fields(data, &["period_start"], Value::is_i64));
    if let Some(limit) = data["extraUsageCap"]["limit"].as_f64() {
        root.insert("extraUsageCap".into(), json!({"limit":limit,"unit":"usd"}));
    }
    if let Some(items) = data["resets"].as_array() {
        root.insert(
            "resets".into(),
            Value::Array(
                items
                    .iter()
                    .map(|r| {
                        let mut entry = fields(r, &["kind"], Value::is_string);
                        entry.extend(fields(r, &["expiresAt", "remaining"], Value::is_number));
                        entry.extend(fields(r, &["usable", "givesBackWeekly"], Value::is_boolean));
                        if let Some(scopes) = r["clears"].as_array() {
                            entry.insert(
                                "clears".into(),
                                json!(scopes.iter().filter_map(Value::as_str).collect::<Vec<_>>()),
                            );
                        }
                        Value::Object(entry)
                    })
                    .collect(),
            ),
        );
    }
    if data["daily"].is_object() {
        root.insert("daily".into(), daily(&data["daily"]));
    }
    if data["history"].is_object() {
        root.insert("history".into(), history(&data["history"]));
    }
    if data["activity"].is_object() {
        root.insert("activity".into(), activity(&data["activity"]));
    }
    if data["period_spend"].is_object() {
        let sums = ["today", "thisWeek", "thisMonth"]
            .into_iter()
            .filter_map(|id| Some((id.to_owned(), numbers(data["period_spend"].get(id)?))))
            .collect();
        root.insert("period_spend".into(), Value::Object(sums));
    }
    if let Some(resources) = root.get_mut("resources").and_then(Value::as_object_mut) {
        for (key, r) in data["resources"].as_object().into_iter().flatten() {
            if r["kind"] == "history" {
                resources.insert(
                    key.clone(),
                    Value::Object(fields(r, &["unit", "kind"], Value::is_string)),
                );
            }
            let Some(item) = resources.get_mut(key).and_then(Value::as_object_mut) else {
                continue;
            };
            item.extend(fields(r, &["label", "model", "window"], Value::is_string));
            item.extend(fields(r, &["amount"], Value::is_number));
            item.extend(fields(
                r,
                &["monthly_charge_limit_enabled"],
                Value::is_boolean,
            ));
        }
    }
}

fn day_lines(report: &str, source: &Value, out: &mut Vec<String>) {
    for (date, day) in source.as_object().into_iter().flatten() {
        let counters = |v: &Value| {
            v.as_object()
                .into_iter()
                .flatten()
                .map(|(key, n)| format!("{key} {n}"))
                .collect::<Vec<_>>()
                .join(" · ")
        };
        let totals = counters(&day["totals"]);
        if totals.is_empty() && day["by_model"].as_object().is_none_or(Map::is_empty) {
            continue;
        }
        out.push(format!("{report} · {date} · {totals}"));
        for (model, counts) in day["by_model"].as_object().into_iter().flatten() {
            out.push(format!("  {} · {}", plain(model), counters(counts)));
        }
    }
}

pub fn details_account(
    row: StoredSnapshot,
    index: usize,
    settings: &DashboardSettings,
    failure: Option<&RefreshFailure>,
    now: Timestamp,
) -> Value {
    let mut out = snapshot_account(&row.0, row.1, row.2.as_ref(), now.0);
    if let Some(data) = row.2.as_ref() {
        enrich(&mut out["usage"], data);
    }
    if let Some(resets) = out
        .get_mut("usage")
        .and_then(|usage| usage.get_mut("resets"))
        .and_then(Value::as_array_mut)
    {
        for grant in resets {
            if grant["expiresAt"].as_i64().is_some_and(|at| at <= now.0) {
                grant["usable"] = false.into();
            }
        }
    }
    let mut display_data = out["usage"].clone();
    if let Some(branches) = display_data.get("daily").cloned() {
        for name in ["consumption", "bills"] {
            if branches[name].is_object()
                && let Some(resources) = display_data["resources"].as_object_mut()
            {
                crate::connected::project_daily(resources, name, &branches[name]);
            }
        }
    }
    let view = (!display_data.is_null()).then_some(display_data);
    let detail = crate::presentation::detail((row.0, row.1, view), now, settings);
    out["account"]["label"] = if settings.conceal_names {
        crate::dashboard::concealed_label(index)
    } else {
        detail.label
    }
    .into();
    let mut lines = detail.details;
    if let Some(f) = failure {
        out["stale"] = true.into();
        out["failure"] = json!({"kind":f.kind,"retry_at":f.retry_at});
        lines.insert(
            0,
            format!(
                "{} · retry {}",
                plain(&f.message),
                Timestamp(f.retry_at).rfc3339()
            ),
        );
    }
    for (index, reset) in out["usage"]["resets"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let status = if reset["usable"] == true {
            "Ready"
        } else {
            "Unavailable"
        };
        let kind = reset["kind"].as_str().unwrap_or("reset");
        let expiry = reset["expiresAt"]
            .as_i64()
            .map(Timestamp)
            .map_or_else(|| "unknown".into(), Timestamp::rfc3339);
        lines.push(format!(
            "Reset {} · {} · {status} · expires {expiry}",
            index + 1,
            plain(kind)
        ));
    }
    if let Some(method) = out["usage"]["authMethod"].as_str() {
        lines.push(format!("Authentication · {}", plain(method)));
    }
    if let Some(at) = out["usage"]["period_start"].as_i64() {
        lines.push(format!("Period started · {}", Timestamp(at).rfc3339()));
    }
    for report in ["counts", "credits"] {
        day_lines(report, &out["usage"]["history"][report]["days"], &mut lines);
    }
    day_lines("Activity", &out["usage"]["activity"]["days"], &mut lines);
    out["details"] = json!(lines);
    out
}
