// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use chrono::DateTime;
use serde_json::{Value, json};

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
                    let expiry = grant["expiresAt"]
                        .as_i64()
                        .and_then(|t| DateTime::from_timestamp(t, 0));
                    metric.detail.push(expiry.map_or_else(
                        || status.into(),
                        |t| format!("{status} · expires {}", t.format("%Y-%m-%d %H:%M UTC")),
                    ));
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
            readings.extend(history(id, range).map(|measure| (id.clone(), measure)));
        }
    }
    readings
}

fn history(id: &str, range: &Value) -> Option<Measure> {
    if !matches!(
        id,
        "today" | "yesterday" | "last30" | "week" | "month" | "prior30"
    ) {
        return None;
    }
    let totals = &range["totals"];
    let (field, unit) = [
        ("tokens", "tokens"),
        ("usageValue", "usd"),
        ("spend", "usd"),
        ("turns", "turns"),
    ]
    .into_iter()
    .find(|(k, _)| totals[*k].is_number())
    .unwrap_or(("tokens", "tokens"));
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
            if v.is_number() {
                m.detail.push(format!("{k} · {v}"));
            }
        }
    }
    if let Some(models) = range["byModel"].as_object() {
        for (name, values) in models {
            if let Some(fields) = values.as_object() {
                let detail = fields
                    .iter()
                    .filter(|(_, v)| v.is_number())
                    .map(|(k, v)| format!("{v} {k}"))
                    .collect::<Vec<_>>()
                    .join(" · ");
                if !detail.is_empty() {
                    m.detail.push(format!("{name} · {detail}"));
                }
            }
        }
    }

    Some(m)
}

fn decode(id: &str, v: &Value) -> Option<Measure> {
    let kind = v["kind"].as_str()?;
    let mut unit = v
        .get("currency")
        .and_then(Value::as_str)
        .or_else(|| v["unit"].as_str())
        .unwrap_or("count")
        .to_owned();
    let scale = v["exponent"]
        .as_i64()
        .filter(|n| (0..=9).contains(n))
        .map_or(1.0, |n| 10f64.powf(n as f64));
    let amount = |key: &str| v[key].as_f64().map(|n| n / scale);
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
    if let Some(p) = raw_percent
        && (used.is_none() || limit.is_none())
        && !matches!(v["status"].as_str(), Some("unlimited" | "not_in_plan"))
    {
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
    let known = safe_name(id);
    let label = v["label"]
        .as_str()
        .or_else(|| v["model"].as_str())
        .map_or_else(
            || {
                if known == "Metric" {
                    id.to_owned()
                } else {
                    known.to_owned()
                }
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
        );
    let reset = v["resetsAt"].as_i64().or_else(|| v["resets_at"].as_i64());
    let period = v["windowSeconds"]
        .as_i64()
        .or_else(|| v["window_seconds"].as_i64())
        .or_else(|| reset?.checked_sub(v["starts_at"].as_i64()?));
    let exact = |k: &str| (scale <= 1.0 && v[k].is_u64()).then(|| v[k].to_string());
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
        exact_left: exact("remaining").or_else(|| exact("available")),
    })
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
        let precision = if self.unit == "percent" {
            1.0
        } else if self.currency() {
            100.0
        } else {
            10.0
        };
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
        if !self.parts.is_empty() {
            return self
                .parts
                .iter()
                .map(|(u, n)| format_value(u, &number(*n)))
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
            Some(n) => format_value(&self.unit, &exact.clone().unwrap_or_else(|| number(n))),
            None => self
                .detail
                .first()
                .filter(|_| self.kind != "history")
                .cloned()
                .unwrap_or_else(|| "Unavailable".into()),
        }
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

pub(super) fn reset_text(at: i64, now: i64, exact: bool) -> String {
    if at <= now {
        return "Imminent".into();
    }
    if exact {
        return DateTime::from_timestamp(at, 0).map_or_else(
            || "Unknown".into(),
            |t| t.format("%Y-%m-%d %H:%M UTC").to_string(),
        );
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
