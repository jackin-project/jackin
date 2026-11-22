// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use jackin_usage_provider_core::{ProviderError, json_number};
use serde_json::{Map, Value, json};
use time::{Duration, OffsetDateTime};

use super::Read;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Report {
    Counts,
    Team,
    Enterprise,
}

impl Report {
    fn endpoint(self) -> (&'static str, &'static str) {
        match self {
            Self::Counts => (
                "analytics/daily-workspace-usage-counts",
                "group_by=day&workspace_user=true",
            ),
            Self::Team => (
                "usage/daily-workspace-user-token-usage-breakdown",
                "group_by=day",
            ),
            Self::Enterprise => ("usage/daily-workspace-user-credit-usage", "breakdown=model"),
        }
    }

    fn workspace(plan: &str) -> Option<Self> {
        match plan.to_ascii_lowercase().as_str() {
            "team"
            | "self_serve_business_prolite"
            | "self_serve_business_usage_based"
            | "sci"
            | "sci_plus" => Some(Self::Team),
            "business"
            | "ent26"
            | "enterprise_cbp_automation"
            | "enterprise_cbp_usage_based"
            | "enterprise"
            | "edu"
            | "edu_plus"
            | "edu_pro" => Some(Self::Enterprise),
            _ => None,
        }
    }
}

pub(super) async fn read(ctx: &Read<'_>, headers: &[(&str, &str)], plan: &str, out: &mut Value) {
    let Ok(today) = OffsetDateTime::from_unix_timestamp(ctx.now).map(OffsetDateTime::date) else {
        return;
    };
    let span: Vec<String> = (0..30)
        .filter_map(|n| today.checked_sub(Duration::days(n)))
        .map(|d| d.to_string())
        .collect();
    if span.len() != 30 {
        return;
    }
    let counts = ledger(ctx, headers, Report::Counts, &span, "counts").await;
    let credits = if let Some(report) = Report::workspace(plan) {
        Some(ledger(ctx, headers, report, &span, "credits").await)
    } else {
        None
    };
    let mut ranges = Map::new();
    for (name, range) in [
        ("today", &span[..1]),
        ("yesterday", &span[1..2]),
        ("last30", &span[..]),
    ] {
        let mut sums = json!({});
        let mut observed = 0;
        for d in range {
            if let Some(day) = counts["days"].get(d) {
                observed += 1;
                merge(&mut sums, day);
                if let Some(supplement) = credits.as_ref().and_then(|v| v["days"].get(d)) {
                    merge(&mut sums, supplement);
                }
            }
        }
        sums["observedDays"] = json!(observed);
        sums["expectedDays"] = json!(range.len());
        ranges.insert(name.into(), sums);
    }
    out["history"] = json!({"counts":counts,"ranges":ranges});
    if let Some(credits) = credits {
        out["history"]["credits"] = credits;
    }
}

async fn ledger(
    ctx: &Read<'_>,
    headers: &[(&str, &str)],
    report: Report,
    span: &[String],
    key: &str,
) -> Value {
    let prior = &ctx.prior["history"][key];
    let today = &span[0];
    if !ctx.manual
        && prior["today"].as_str() == Some(today)
        && prior["readAt"]
            .as_i64()
            .is_some_and(|at| ctx.now.saturating_sub(at) < 3600)
    {
        return prior.clone();
    }
    let mut days = prior["days"].as_object().cloned().unwrap_or_default();
    days.retain(|d, _| span.contains(d));
    let due: Vec<_> = span
        .iter()
        .enumerate()
        .filter(|(i, d)| *i < 2 || !days.contains_key(*d) || prior["today"].as_str() == Some(*d))
        .map(|(_, d)| d)
        .collect();
    for day in &due {
        days.remove(*day);
    }
    let mut hold = prior["retryAt"].as_i64().filter(|at| *at > ctx.now);
    if hold.is_none() {
        let (path, query) = report.endpoint();
        let url = format!(
            "https://chatgpt.com/backend-api/wham/{path}?start_date={}&end_date={today}&{query}",
            due.last().unwrap_or(&today)
        );
        match ctx.get(&url, headers).await {
            Ok(body) => {
                if let Some((mut readings, quiet)) = parse(&body, report) {
                    for day in due {
                        days.insert(
                            day.clone(),
                            readings.remove(day).unwrap_or_else(|| quiet.clone()),
                        );
                    }
                }
            }
            Err(ProviderError::RateLimited {
                retry_after_seconds,
            }) => {
                hold = Some(ctx.now.saturating_add(
                    i64::try_from(retry_after_seconds.unwrap_or(3600)).unwrap_or(i64::MAX),
                ));
            }
            Err(_) => {}
        }
    }
    let mut result = json!({"days":days,"readAt":ctx.now,"today":today});
    if let Some(t) = hold {
        result["retryAt"] = json!(t);
    }
    result
}

fn parse(body: &Value, report: Report) -> Option<(Map<String, Value>, Value)> {
    let mut days = Map::new();
    for record in body["data"].as_array()? {
        let date = record["date"].as_str()?.get(..10)?;
        let mut day = json!({});
        match report {
            Report::Counts => {
                let totals = &record["totals"];
                day["totals"]["turns"] = json!(nonnegative(&totals["turns"])?);
                for (src, dst) in [
                    ("text_total_tokens", "tokens"),
                    ("uncached_text_input_tokens", "inputTokens"),
                    ("cached_text_input_tokens", "cacheReadTokens"),
                    ("text_output_tokens", "outputTokens"),
                ] {
                    if !totals[src].is_null() {
                        day["totals"][dst] = json!(nonnegative(&totals[src])?);
                    }
                }
                counted_models(&mut day, record);
            }
            Report::Team | Report::Enterprise => {
                let values = match report {
                    Report::Team => &record["premium_usage_values"]["credit_usage_credits"],
                    _ => &record["values"],
                };
                let Some(amounts) = values.as_object() else {
                    return Some((Map::new(), json!({})));
                };
                let mut total = 0.0;
                for (name, value) in amounts {
                    let amount = json_number(value)?;
                    total += amount;
                    if report == Report::Enterprise {
                        add(&mut day["byModel"][name]["credits"], amount);
                    }
                }
                day["totals"]["credits"] = json!(total);
                if report == Report::Team {
                    charged_models(&mut day, record);
                }
            }
        }
        merge(days.entry(date).or_insert(json!({})), &day);
    }
    Some((
        days,
        if report == Report::Counts {
            json!({})
        } else {
            json!({"totals":{"credits":0}})
        },
    ))
}

fn counted_models(day: &mut Value, record: &Value) {
    for model in record["models"].as_array().into_iter().flatten() {
        let name = model["model"].as_str().unwrap_or("Unknown");
        for (src, dst) in [("turns", "turns"), ("text_total_tokens", "tokens")] {
            if let Some(n) = nonnegative(&model[src]) {
                add(&mut day["byModel"][name][dst], n);
            }
        }
    }
}

fn charged_models(day: &mut Value, record: &Value) {
    for model in record["models"].as_array().into_iter().flatten() {
        if let Some(credits) = json_number(&model["on_demand_credits"]) {
            let name = model["model"].as_str().unwrap_or("Unknown");
            add(&mut day["byModel"][name]["credits"], credits);
        }
    }
}

fn nonnegative(v: &Value) -> Option<f64> {
    json_number(v).filter(|n| *n >= 0.0)
}
fn add(value: &mut Value, n: f64) {
    *value = json!(value.as_f64().unwrap_or(0.0) + n);
}
fn merge(out: &mut Value, day: &Value) {
    if let Some(fields) = day.as_object() {
        for (key, value) in fields {
            if let Some(n) = value.as_f64() {
                add(&mut out[key], n);
            } else if value.is_object() {
                merge(&mut out[key], value);
            }
        }
    }
}
