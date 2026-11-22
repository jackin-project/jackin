// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_usage_provider_core::Http;
use jackin_usage_provider_mimo::{DailySource, fetch_daily};
use serde_json::{Map, Value, json};
use time::{Duration, OffsetDateTime, macros::date};

pub(super) async fn read(
    http: &Http,
    key: &str,
    now: i64,
    previous: Option<&Value>,
    manual: bool,
    snapshot: &mut Value,
) {
    let Ok(today) = OffsetDateTime::from_unix_timestamp(now).map(OffsetDateTime::date) else {
        return;
    };
    let Some(start) = today.checked_sub(Duration::days(29)) else {
        return;
    };
    let earliest = date!(2026 - 09 - 01);
    if today < earliest {
        return;
    }
    let start = start.max(earliest);
    for (field, source) in [
        ("consumption", DailySource::Consumption),
        ("bills", DailySource::Bills),
    ] {
        let mut prior = previous
            .and_then(|s| s["daily"].get(field))
            .cloned()
            .unwrap_or_else(|| json!({}));
        let cadence = if prior["unavailable"].as_bool() == Some(true) || manual {
            300
        } else {
            3600
        };
        let fresh = prior["attemptedAt"]
            .as_i64()
            .is_some_and(|t| (0..cadence).contains(&now.saturating_sub(t)));
        if !fresh {
            match fetch_daily(http, key, source, start, today).await {
                Ok(rows) => {
                    prior = json!({"readAt":now,"start":start.to_string(),"end":today.to_string(),"rows":rows});
                }
                Err(_) => prior["unavailable"] = json!(true),
            }
            prior["attemptedAt"] = json!(now);
        }
        if let Some(resources) = snapshot["resources"].as_object_mut() {
            project(resources, field, &prior);
        }
        snapshot["daily"][field] = prior;
    }
}

fn project(resources: &mut Map<String, Value>, source: &str, branch: &Value) {
    let title = if source == "bills" {
        "Daily bills"
    } else {
        "Daily consumption"
    };
    let summary_id = format!("daily/{source}");
    let Some(rows) = branch["rows"].as_array() else {
        resources.insert(
            summary_id,
            json!({"kind":"history","label":title,"details":["Daily history unavailable"]}),
        );
        return;
    };
    let mut details = vec![format!(
        "{} to {} UTC",
        branch["start"].as_str().unwrap_or_default(),
        branch["end"].as_str().unwrap_or_default()
    )];
    if branch["unavailable"] == true {
        details.push("Latest read failed · showing saved history".into());
    }
    let mut totals = BTreeMap::<&str, i128>::new();
    for row in rows {
        let currency = row["currency"].as_str().unwrap_or_default();
        let field = if source == "bills" {
            "final_paid_amount"
        } else {
            "consumed_amount"
        };
        let Some(amount) = row[field].as_i64() else {
            continue;
        };
        *totals.entry(currency).or_default() += i128::from(amount);
        let mut line = format!(
            "{} · {} {} {}",
            row["date"].as_str().unwrap_or_default(),
            row["name"].as_str().unwrap_or("Payment"),
            decimal(i128::from(amount), source),
            currency
        );
        for (field, title) in [
            ("request_count", "requests"),
            ("total_tokens", "tokens"),
            ("audio_transcription_seconds", "audio seconds"),
        ] {
            if let Some(n) = row[field].as_u64() {
                line.push_str(&format!(" · {n} {title}"));
            }
        }
        details.push(line);
    }
    for (currency, n) in totals {
        details.insert(1, format!("Total {} {currency}", decimal(n, source)));
    }
    resources.insert(summary_id,json!({"kind":"history","unit":"records","label":title,"amount":rows.len(),"details":details}));
}

fn decimal(n: i128, source: &str) -> String {
    let precision = if source == "bills" { 2 } else { 6 };
    let scale = if source == "bills" { 100 } else { 1_000_000 };
    let amount = n.unsigned_abs();
    let sign = if n < 0 { "-" } else { "" };
    format!("{sign}{}.{:0precision$}", amount / scale, amount % scale)
}
