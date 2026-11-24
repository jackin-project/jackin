// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use jackin_usage_provider_core::Decimal;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use time::OffsetDateTime;

use super::{DashboardSettings, Severity};

#[derive(Clone, Debug)]
pub(super) struct Measure {
    pub label: String,
    pub unit: String,
    pub kind: String,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub available: Option<f64>,
    pub reset: Option<i64>,
    pub period: Option<i64>,
    pub empty_balance: bool,
    pub parts: Vec<(String, f64)>,
    pub detail: Vec<String>,
    exact_limit: Option<String>,
    exact_used: Option<String>,
    exact_left: Option<String>,
    exhausted: Option<bool>,
}

#[derive(Clone, Copy)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent threshold and window conditions may coexist."
)]
pub(super) struct Metrics {
    pub bounded: bool,
    pub spent: bool,
    pub severity: Severity,
    pub remaining: f64,
    pub share: Option<f64>,
    pub bucket: i8,
    pub burning: bool,
    pub unused: bool,
    pub runway: Option<f64>,
}

const LABELS: &[(&str, &str)] = &[
    ("session", "Session"),
    ("weekly", "Weekly"),
    ("gemini-5h", "Gemini 5h"),
    ("gemini-weekly", "Gemini weekly"),
    ("gemini-argon-wtu-weekly", "Argon weekly"),
    ("3p-5h", "Claude 5h"),
    ("3p-weekly", "Claude weekly"),
    ("monthly", "Monthly"),
    ("monthlyTotal", "Monthly total"),
    ("monthlyCode", "Monthly code"),
    ("spark", "Spark"),
    ("sparkWeekly", "Spark weekly"),
    ("sonnet", "Sonnet"),
    ("opus", "Opus"),
    ("fable", "Fable"),
    ("extraUsage", "Extra usage"),
    ("credits", "Credits"),
    ("creditLimit", "Credit limit"),
    ("balance", "Balance"),
    ("rateLimitResets", "Limit resets"),
    ("webSearches", "Web searches"),
    ("keyLimit", "Key limit"),
    ("budget", "Budget"),
    ("freeRequests", "Free requests"),
    ("tokenPlan", "Token Plan"),
    ("booster", "Booster"),
    ("today", "Today"),
    ("yesterday", "Yesterday"),
    ("last30", "Last 30 days"),
    ("week", "This week"),
    ("month", "This month"),
    ("prior30", "Prior 30 days"),
    ("payAsYouGo", "Extra usage cap"),
    ("monthlyChargeLimit", "Monthly charge limit"),
    ("monthlyChargeUsed", "Monthly charges"),
];

pub(super) fn safe_name(id: &str) -> &str {
    LABELS
        .iter()
        .find(|(key, _)| *key == id)
        .map_or("Metric", |(_, label)| *label)
}
pub(super) fn default_order(id: &str) -> usize {
    LABELS
        .iter()
        .position(|(key, _)| *key == id)
        .unwrap_or(usize::MAX)
}

pub(super) fn read(body: &Value) -> Vec<(String, Measure)> {
    let mut readings: Vec<_> = body["resources"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, resource)| {
            let mut metric = decode(id, resource)?;
            if id == "rateLimitResets" {
                for grant in body["resets"].as_array().into_iter().flatten() {
                    let status = if grant["usable"] == true {
                        "Ready"
                    } else {
                        "Unavailable"
                    };
                    let expiry = grant["expiresAt"].as_i64().and_then(utc_minute);
                    metric.detail.push(
                        expiry.map_or_else(|| status.into(), |t| format!("{status} · expires {t}")),
                    );
                }
            }
            Some((id.clone(), metric))
        })
        .collect();
    if let Some(cap) = body["extraUsageCap"]["limit"].as_f64()
        && let Some(m) = decode(
            "payAsYouGo",
            &json!({"kind":"amount","unit":"usd","amount":cap}),
        )
    {
        readings.push(("payAsYouGo".into(), m));
    }
    if let Some(ranges) = body["history"]["ranges"].as_object() {
        for (id, range) in ranges {
            readings.extend(history(id, range, false).map(|measure| (id.clone(), measure)));
        }
    }
    for (id, key) in [
        ("today", "today"),
        ("week", "thisWeek"),
        ("month", "thisMonth"),
    ] {
        if let Some(totals) = body["period_spend"].get(key) {
            readings
                .extend(history(id, &json!({"totals":totals}), true).map(|m| (id.to_owned(), m)));
        }
    }
    let activity = &body["activity"];
    if activity.is_object() {
        let yesterday = &activity["yesterday"];
        let prior = json!({"totals":activity["prior30Days"], "by_model":prior_models(&activity["days"]),"observedDays":activity["observed_days"],"expectedDays":activity["expected_days"]});
        for (id, range) in [("yesterday", yesterday), ("prior30", &prior)] {
            readings.extend(history(id, range, true).map(|m| (id.to_owned(), m)));
        }
    }
    readings
}

fn prior_models(days: &Value) -> Value {
    let mut models: BTreeMap<&str, BTreeMap<&str, Option<Decimal>>> = BTreeMap::new();
    for (_, day) in days.as_object().into_iter().flatten() {
        for (name, fields) in day["by_model"].as_object().into_iter().flatten() {
            for (field, value) in fields.as_object().into_iter().flatten() {
                if let Ok(amount) = Decimal::from_json(value) {
                    let sum = models
                        .entry(name)
                        .or_default()
                        .entry(field)
                        .or_insert(Some(Decimal::ZERO));
                    *sum = sum.and_then(|total| total.checked_add(&amount));
                }
            }
        }
    }
    Value::Object(
        models
            .into_iter()
            .map(|(name, fields)| {
                let fields = fields
                    .into_iter()
                    .filter_map(|(field, sum)| {
                        let sum = sum?;
                        let shown = if matches!(field, "spend" | "byokSpend") {
                            Value::String(sum.to_string())
                        } else {
                            json!(sum.to_f64())
                        };
                        Some((field.to_owned(), shown))
                    })
                    .collect();
                (name.to_owned(), Value::Object(fields))
            })
            .collect(),
    )
}

fn history_quantity(field: &str, value: &Value) -> Option<String> {
    let amount = match value {
        Value::String(text) => Decimal::parse_text(text).ok()?.to_string(),
        other => number(other.as_f64()?),
    };
    let unit = match field {
        "spend" | "usageValue" => "USD spent",
        "byokSpend" => "USD BYOK spend",
        "inputTokens" => "input tokens",
        "cacheReadTokens" => "cached input tokens",
        "outputTokens" => "output tokens",
        "reasoningTokens" => "reasoning tokens",
        other => other,
    };
    Some(format!("{amount} {unit}"))
}

fn history(id: &str, range: &Value, spend: bool) -> Option<Measure> {
    if !matches!(
        id,
        "today" | "yesterday" | "last30" | "week" | "month" | "prior30"
    ) {
        return None;
    }
    let totals = &range["totals"];
    let (field, unit) = if spend {
        ("spend", "usd")
    } else {
        [
            ("tokens", "tokens"),
            ("usageValue", "usd"),
            ("spend", "usd"),
            ("turns", "turns"),
        ]
        .into_iter()
        .find(|(k, _)| totals[*k].is_number())
        .unwrap_or(("tokens", "tokens"))
    };
    let mut m = decode(
        id,
        &json!({"kind":"history","unit":unit,"amount":totals[field]}),
    )?;
    if let Some((observed, expected)) = range["observedDays"]
        .as_u64()
        .zip(range["expectedDays"].as_u64())
    {
        m.detail
            .push(format!("{observed} of {expected} days observed"));
    }
    if let Some(totals) = totals.as_object() {
        for (k, v) in totals {
            m.detail.extend(history_quantity(k, v));
        }
    }
    if let Some(models) = range
        .get("byModel")
        .or_else(|| range.get("by_model"))
        .and_then(Value::as_object)
    {
        for (name, values) in models {
            if let Some(fields) = values.as_object()
                && !fields.is_empty()
            {
                m.detail.push(name.clone());
                m.detail
                    .extend(fields.iter().filter_map(|(k, v)| history_quantity(k, v)));
            }
        }
    }
    Some(m)
}

fn metric_label(id: &str, v: &Value) -> String {
    let known = safe_name(id);
    v["label"]
        .as_str()
        .or_else(|| v["model"].as_str())
        .map_or_else(
            || {
                if known != "Metric" {
                    return known.to_owned();
                }
                if let Some((prefix, field)) = id.split_once('/')
                    && v["currency"].as_str() == Some(prefix)
                {
                    let balance = match field {
                        "cash" => "Cash",
                        "gift" => "Gift balance",
                        "frozen" => "Frozen balance",
                        "creditLimit" => "Credit limit",
                        "creditAvailable" => "Available credit",
                        "totalAvailable" => "Total available",
                        _ => return id.to_owned(),
                    };
                    return format!("{balance} ({prefix})");
                }
                id.to_owned()
            },
            |label| {
                if let Some(window) = v["window"].as_str() {
                    format!("{label} {window}")
                } else if let Some(token) = v["unit"].as_str().filter(|_| v["model"].is_string()) {
                    format!("{label} {token}")
                } else {
                    label.to_owned()
                }
            },
        )
}

fn scaled_integer(field: &Value, exponent: u32) -> Option<String> {
    let n = field
        .as_i64()
        .map(i128::from)
        .or_else(|| field.as_u64().map(i128::from))?;
    let scale = 10_i128.pow(exponent);
    if n % scale == 0 {
        return Some((n / scale).to_string());
    }
    let sign = if n < 0 { "-" } else { "" };
    let value = n.abs();
    let decimals = format!("{:0width$}", value % scale, width = exponent as usize);
    Some(format!(
        "{sign}{}.{}",
        value / scale,
        decimals.trim_end_matches('0')
    ))
}

fn decode(id: &str, v: &Value) -> Option<Measure> {
    let kind = v["kind"].as_str()?;
    let mut unit = v
        .get("currency")
        .and_then(Value::as_str)
        .or_else(|| v["unit"].as_str())
        .unwrap_or("count")
        .to_owned();
    let exponent = if unit == "millionths_of_cent" {
        unit = "cents".into();
        6
    } else {
        v["exponent"].as_u64().filter(|n| *n <= 9).unwrap_or(0) as u32
    };
    let scale = 10_u64.pow(exponent) as f64;
    let amount = |key: &str| match &v[key] {
        Value::String(text) => Decimal::parse_text(text).ok().map(|exact| exact.to_f64()),
        other => other.as_f64().map(|n| n / scale),
    };
    let mut used = amount("used");
    let mut limit = amount("limit");
    let mut available = amount("available")
        .or_else(|| amount("amount"))
        .or_else(|| amount("remaining"));
    let raw_percent = v["remaining_percent"]
        .as_f64()
        .or_else(|| v["remaining_fraction"].as_f64().map(|p| p * 100.0));
    let mut detail: Vec<String> = v["details"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if let Some(enabled) = v["monthly_charge_limit_enabled"].as_bool() {
        detail.push(format!(
            "Monthly charge limit · {}",
            if enabled { "On" } else { "Off" }
        ));
    }
    if let Some(boost) = v["boost_permille"].as_i64()
        && boost != 1000
    {
        detail.push(format!(
            "Quota multiplier · {}×",
            number(boost as f64 / 1000.0)
        ));
        if let Some(base) = v["base_remaining_percent"].as_f64() {
            detail.push(format!("Base quota remaining · {}%", number(base)));
        }
        if let Some(effective) = raw_percent {
            detail.push(format!("Boosted quota remaining · {}%", number(effective)));
        }
    }
    let fraction = raw_percent.filter(|_| {
        (used.is_none() || limit.is_none())
            && !matches!(v["status"].as_str(), Some("unlimited" | "not_in_plan"))
    });
    if let Some(p) = fraction {
        if let Some(n) = available {
            detail.push(format!("Remaining {} {unit}", number(n)));
        }
        unit = "percent".into();
        used = Some((100.0 - p).max(0.0));
        limit = Some(100.0);
        available = Some(p);
    }
    if matches!(v["status"].as_str(), Some("unlimited" | "not_in_plan")) {
        used = None;
        limit = None;
        available = None;
        detail.push(if v["status"] == "unlimited" {
            "Unlimited".into()
        } else {
            "Outside plan".into()
        });
    }
    let label = metric_label(id, v);
    let reset = v["resetsAt"].as_i64().or_else(|| v["resets_at"].as_i64());
    let period = v["windowSeconds"]
        .as_i64()
        .or_else(|| v["window_seconds"].as_i64())
        .or_else(|| reset?.checked_sub(v["starts_at"].as_i64()?));
    let exact = |key: &str| {
        fraction
            .is_none()
            .then(|| match &v[key] {
                Value::String(text) => Decimal::parse_text(text)
                    .ok()
                    .map(|exact| exact.to_string()),
                other => scaled_integer(other, exponent),
            })
            .flatten()
    };
    let empty_balance = kind == "balance"
        && id != "rateLimitResets"
        && !id.to_lowercase().contains("reset")
        && available == Some(0.0)
        && (!id.contains('/') || id.ends_with("/totalAvailable"));
    Some(Measure {
        label,
        unit,
        kind: kind.into(),
        used,
        limit,
        available,
        reset,
        period,
        empty_balance,
        parts: Vec::new(),
        detail,
        exact_limit: exact("limit"),
        exact_used: exact("used"),
        exact_left: exact("available")
            .or_else(|| exact("amount"))
            .or_else(|| exact("remaining")),
        exhausted: exhausted(v).filter(|_| fraction.is_none()),
    })
}

fn exhausted(v: &Value) -> Option<bool> {
    let (Value::String(limit), Value::String(used)) = (&v["limit"], &v["used"]) else {
        return None;
    };
    let (limit, used) = Decimal::parse_text(limit)
        .ok()
        .zip(Decimal::parse_text(used).ok())?;
    limit.checked_sub(&used).map(|left| !left.is_positive())
}

pub(super) fn advanced(reset: Option<i64>, previous: Option<i64>, now: i64) -> bool {
    reset.is_some_and(|t| previous.is_none_or(|old| old <= now && t.saturating_sub(old) > 1))
}

impl Measure {
    pub(super) fn bounded(&self) -> bool {
        self.limit.is_some_and(|l| l > 0.0) && self.used.is_some()
    }
    fn currency(&self) -> bool {
        self.unit.len() == 3
            && self.unit.bytes().all(|c| c.is_ascii_alphabetic())
            && self.unit != "day"
    }
    pub(super) fn spent(&self) -> bool {
        if !self.bounded() {
            return false;
        }
        if let Some(exhausted) = self.exhausted {
            return exhausted;
        }
        if self.unit == "percent" {
            return self
                .available
                .unwrap_or_else(|| self.limit.unwrap_or_default() - self.used.unwrap_or_default())
                <= 0.0;
        }
        let precision = if self.currency() { 100.0 } else { 10.0 };
        ((self.limit.unwrap_or_default() - self.used.unwrap_or_default()) * precision).round()
            <= 0.0
    }

    pub(super) fn metrics(
        &self,
        settings: &DashboardSettings,
        now: i64,
        previous: Option<i64>,
    ) -> Metrics {
        let bounded = self.bounded();
        let frac = if bounded {
            (self.used.unwrap_or_default() / self.limit.unwrap_or(1.0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let remaining = 1.0 - frac;
        let p = (100.0 * remaining).round();
        let severity = if !bounded {
            Severity::Normal
        } else if p <= f64::from(settings.critical) {
            Severity::Critical
        } else if p <= f64::from(settings.warning) {
            Severity::Warning
        } else {
            Severity::Normal
        };
        let share = self.reset.zip(self.period).and_then(|(reset, period)| {
            let elapsed = now.saturating_sub(reset.saturating_sub(period)) as f64;
            (period > 0 && reset > now && elapsed >= (0.01 * period as f64).max(60.0))
                .then(|| elapsed / period as f64)
        });
        let pace = share
            .map(|s| frac / s)
            .filter(|&p| frac > 0.0 && (p <= 0.9 || frac >= 0.05));
        let spent = self.spent();
        let bucket = if spent {
            2
        } else {
            pace.map_or(-1, |p| {
                if p <= 0.9 {
                    0
                } else if ((1.0 - p) * 100.0).round() >= 1.0 {
                    1
                } else {
                    2
                }
            })
        };
        let burning = !spent
            && (bucket == 2
                || bucket == 1 && previous.is_some() && !advanced(self.reset, previous, now));
        let unused = bounded
            && !self.currency()
            && self.period.is_some_and(|p| p > 86400)
            && self
                .reset
                .is_some_and(|t| (1..=86400).contains(&t.saturating_sub(now)))
            && p > 50.0;
        Metrics {
            bounded,
            spent,
            severity,
            remaining,
            share,
            bucket,
            burning,
            unused,
            runway: pace
                .zip(self.period)
                .map(|(p, period)| remaining * period as f64 / p),
        }
    }

    pub(super) fn display(&self, remaining: bool) -> String {
        self.display_amount(remaining, false)
    }

    pub(super) fn compact_display(&self, remaining: bool) -> String {
        self.display_amount(remaining, true)
    }

    fn display_amount(&self, remaining: bool, compact: bool) -> String {
        if !self.parts.is_empty() {
            return self
                .parts
                .iter()
                .map(|(u, n)| display_quantity(u, *n, None, compact))
                .collect::<Vec<_>>()
                .join(" · ");
        }
        let (value, exact) = if self.bounded() {
            if remaining {
                (
                    self.available
                        .or_else(|| self.limit.zip(self.used).map(|(l, u)| (l - u).max(0.0))),
                    &self.exact_left,
                )
            } else {
                (self.used, &self.exact_used)
            }
        } else if self.available.is_some() {
            (self.available, &self.exact_left)
        } else {
            (self.used, &self.exact_used)
        };
        match value {
            Some(n) if self.unit == "percent" && n > 0.0 && n < 1.0 => "<1%".into(),
            Some(n) if self.unit == "percent" && n > 99.0 && n < 100.0 => ">99%".into(),
            Some(n) => display_quantity(&self.unit, n, exact.as_deref(), compact),
            None => self
                .detail
                .first()
                .filter(|_| self.kind != "history")
                .cloned()
                .unwrap_or_else(|| "Unavailable".into()),
        }
    }

    pub(super) fn quantity_details(&self) -> Vec<String> {
        let mut details = Vec::new();
        if self.bounded() {
            details.push(format!("Used · {}", self.display(false)));
            if let Some(limit) = self.limit {
                details.push(format!(
                    "Limit · {}",
                    format_value(
                        &self.unit,
                        &self.exact_limit.clone().unwrap_or_else(|| number(limit))
                    )
                ));
            }
            details.push(format!("Remaining · {}", self.display(true)));
        }
        details
    }

    pub(super) fn caption(&self, remaining: bool) -> String {
        let value = self.compact_display(remaining);
        if self.bounded() {
            format!("{value} {}", if remaining { "remaining" } else { "used" })
        } else {
            value
        }
    }

    pub(super) fn inspector_limit(&self, compact: bool) -> Option<String> {
        if self.unit == "percent" || !self.bounded() {
            return None;
        }
        self.limit
            .map(|limit| display_quantity(&self.unit, limit, self.exact_limit.as_deref(), compact))
    }

    pub(super) fn clear_exact(&mut self) {
        self.exact_limit = None;
        self.exact_used = None;
        self.exact_left = None;
    }

    pub(super) fn redacted_values(&self) -> Value {
        let safe_unit = |s: &str| {
            matches!(
                s,
                "percent"
                    | "count"
                    | "credits"
                    | "resets"
                    | "requests"
                    | "tokens"
                    | "searches"
                    | "usd"
                    | "eur"
                    | "cny"
                    | "USD"
                    | "EUR"
                    | "CNY"
            )
            .then_some(s.to_owned())
        };
        let quantity = |exact: &Option<String>, fallback: Option<f64>| {
            exact
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok())
                .map_or_else(|| json!(fallback), |n| json!(n))
        };
        json!({"unit":safe_unit(&self.unit),"used":quantity(&self.exact_used,self.used),"limit":quantity(&self.exact_limit,self.limit),"available":quantity(&self.exact_left,self.available),
            "amounts":self.parts.iter().map(|(u,n)|json!({"unit":safe_unit(u),"amount":n})).collect::<Vec<_>>()})
    }
}

fn number(n: f64) -> String {
    format!("{n:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn display_quantity(unit: &str, amount: f64, exact: Option<&str>, compact: bool) -> String {
    if compact
        && matches!(
            unit,
            "count"
                | "credits"
                | "requests"
                | "REQUESTS"
                | "resets"
                | "searches"
                | "tokens"
                | "turns"
                | "usd"
                | "USD"
                | "eur"
                | "EUR"
                | "cny"
                | "CNY"
        )
        && amount.abs() >= 10_000.0
    {
        let (scale, suffix) = [
            (1_000_000_000_000.0, "T"),
            (1_000_000_000.0, "B"),
            (1_000_000.0, "M"),
            (1_000.0, "K"),
        ]
        .into_iter()
        .find(|(scale, _)| amount.abs() >= scale * 0.999_995)
        .unwrap_or((1_000.0, "K"));
        return format!("{}{suffix} {unit}", number(amount / scale));
    }
    match exact {
        Some(exact) => format_value(unit, exact),
        None => format_value(unit, &number(amount)),
    }
}

fn format_value(unit: &str, n: &str) -> String {
    if unit == "percent" {
        format!(
            "{}%",
            n.parse::<f64>()
                .map_or_else(|_| n.to_owned(), |v| v.round().to_string())
        )
    } else {
        format!("{n} {unit}")
    }
}

fn utc_minute(at: i64) -> Option<String> {
    let utc = OffsetDateTime::from_unix_timestamp(at).ok()?;
    Some(format!(
        "{} {:02}:{:02} UTC",
        utc.date(),
        utc.hour(),
        utc.minute()
    ))
}

pub(super) fn reset_text(at: i64, now: i64, exact: bool) -> String {
    if at <= now {
        return "Imminent".into();
    }
    if exact {
        return utc_minute(at).unwrap_or_else(|| "Unknown".into());
    }
    let seconds = at.saturating_sub(now);
    if seconds >= 86400 {
        format!("{}d {}h", seconds / 86400, (seconds % 86400) / 3600)
    } else if seconds >= 3600 {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    } else {
        format!("{}m", seconds.div_euclid(60).max(1))
    }
}
