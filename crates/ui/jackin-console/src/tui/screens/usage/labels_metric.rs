// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage metric value labels.

use super::{UsageAccount, UsageMetricGroup, UsageWindow, relative_time_label, updated_age_label};
use std::time::{SystemTime, UNIX_EPOCH};

use jackin_protocol::usage_broker::{
    UsageCalendarPeriodV1, UsageFreshnessPhaseV1, UsageIssueV1, UsageMetricPeriodV1,
    UsageMetricScopeV1, UsageMetricValueV1, UsagePercent,
};

pub(crate) fn duration_label(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3_600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3_600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

pub(crate) fn metric_period_label(period: &UsageMetricPeriodV1) -> Option<String> {
    match period {
        UsageMetricPeriodV1::Rolling { window_secs } => {
            Some(format!("rolling {}", duration_label(*window_secs)))
        }
        UsageMetricPeriodV1::Calendar { granularity } => Some(
            match granularity {
                UsageCalendarPeriodV1::Daily => "daily",
                UsageCalendarPeriodV1::Weekly => "weekly",
                UsageCalendarPeriodV1::Monthly => "monthly",
            }
            .to_owned(),
        ),
        UsageMetricPeriodV1::ProviderDefined => Some("provider-defined period".to_owned()),
        UsageMetricPeriodV1::Unknown => None,
    }
}

/// One percent side (`remaining` or `used`) as display text. The raw provider
/// value rides along only when it differs from clamped geometry, so overage
/// stays honest without duplicating equal values.
pub(crate) fn percent_side_summary(
    clamped: Option<u8>,
    raw: Option<i32>,
    word: &str,
) -> Option<String> {
    match (clamped, raw) {
        (Some(percent), Some(raw)) if i32::from(percent) != raw => {
            Some(format!("{percent}% {word} (raw {raw}%)"))
        }
        (Some(percent), _) => Some(format!("{percent}% {word}")),
        (None, Some(raw)) => Some(format!("raw {raw}% {word}")),
        (None, None) => None,
    }
}

pub(crate) fn window_percent_summary(
    remaining: Option<u8>,
    remaining_raw: Option<i32>,
    used: Option<u8>,
    used_raw: Option<i32>,
) -> Option<String> {
    // "left" matches the principal-window value labels (projection-owned)
    // and the Capsule bucket presentation; "remaining" would be a third word
    // for the same meaning (S4/S5 parity).
    percent_side_summary(remaining, remaining_raw, "left")
        .or_else(|| percent_side_summary(used, used_raw, "used"))
}

/// Raw-percent note for a principal window, shown only when a raw value is
/// present and differs from the clamped geometry the bar uses.
pub(crate) fn raw_percent_note(window: &UsageWindow) -> Option<String> {
    for (clamped, raw, word) in [
        (
            window.remaining_percent,
            window.remaining_raw_percent,
            "remaining",
        ),
        (window.used_percent, window.used_raw_percent, "used"),
    ] {
        if let Some(raw) = raw
            && clamped.is_none_or(|percent| i32::from(percent) != raw)
        {
            return Some(format!("raw {word} {raw}%"));
        }
    }
    None
}

/// One-line typed value summary for a metric group. `None` means the provider
/// supplied no displayable value — callers render no value line at all rather
/// than a fabricated zero.
pub(crate) fn metric_group_value_summary(group: &UsageMetricGroup) -> Option<String> {
    match &group.value {
        UsageMetricValueV1::Window {
            remaining_percent,
            remaining_raw_percent,
            used_percent,
            used_raw_percent,
            period,
            unit,
        } => {
            let mut parts = Vec::new();
            if let Some(percent) = window_percent_summary(
                remaining_percent.map(UsagePercent::get),
                *remaining_raw_percent,
                used_percent.map(UsagePercent::get),
                *used_raw_percent,
            ) {
                parts.push(percent);
            }
            if let Some(unit) = unit.as_deref().filter(|unit| !unit.trim().is_empty()) {
                parts.push((*unit).to_owned());
            }
            if let Some(period) = metric_period_label(period) {
                parts.push(period);
            }
            (!parts.is_empty()).then(|| parts.join(" · "))
        }
        UsageMetricValueV1::Balance { amount, .. } => Some(amount.to_string()),
        UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            let mut parts = Vec::new();
            match cap {
                Some(cap) => parts.push(format!("cap {cap}")),
                None => parts.push("uncapped".to_owned()),
            }
            if let Some(spent) = spent {
                parts.push(format!("spent {spent}"));
            }
            if let Some(remaining) = remaining {
                parts.push(format!("remaining {remaining}"));
            }
            Some(parts.join(" · "))
        }
        UsageMetricValueV1::TokenTotals {
            input,
            output,
            cached,
            reasoning,
            interval_label,
        } => {
            let mut parts = Vec::new();
            for (count, word) in [
                (*input, "input"),
                (*output, "output"),
                (*cached, "cached"),
                (*reasoning, "reasoning"),
            ] {
                if let Some(count) = count {
                    parts.push(format!("{word} {count}"));
                }
            }
            if let Some(label) = interval_label
                .as_deref()
                .filter(|label| !label.trim().is_empty())
            {
                parts.push((*label).to_owned());
            }
            (!parts.is_empty()).then(|| parts.join(" · "))
        }
        UsageMetricValueV1::RateLimit {
            limit,
            remaining,
            window_label,
        } => {
            let mut parts = Vec::new();
            if let Some(limit) = limit {
                parts.push(format!("limit {limit}"));
            }
            if let Some(remaining) = remaining {
                parts.push(format!("remaining {remaining}"));
            }
            if let Some(label) = window_label
                .as_deref()
                .filter(|label| !label.trim().is_empty())
            {
                parts.push((*label).to_owned());
            }
            (!parts.is_empty()).then(|| parts.join(" · "))
        }
        UsageMetricValueV1::Plan { plan_label, tier } => {
            let mut parts = Vec::new();
            if let Some(label) = plan_label
                .as_deref()
                .filter(|label| !label.trim().is_empty())
            {
                parts.push((*label).to_owned());
            }
            if let Some(tier) = tier.as_ref().filter(|tier| !tier.trim().is_empty()) {
                parts.push(format!("tier {tier}"));
            }
            (!parts.is_empty()).then(|| parts.join(" · "))
        }
    }
}

/// Non-secret scope labels locating a group inside its account. `None` means
/// the provider did not scope the group on any axis.
pub(crate) fn metric_scope_summary(scope: &UsageMetricScopeV1) -> Option<String> {
    let mut parts = Vec::new();
    for (label, word) in [
        (&scope.service, "service"),
        (&scope.model, "model"),
        (&scope.pool, "pool"),
        (&scope.key_id, "key"),
    ] {
        if let Some(label) = label.as_ref().filter(|label| !label.trim().is_empty()) {
            parts.push(format!("{word} {label}"));
        }
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// One sanitized issue as display text: the Rust-owned operator message with
/// its stable code, plus a broker-owned retry time when one is present.
pub(crate) fn issue_text(issue: &UsageIssueV1, now_epoch: i64) -> String {
    let mut text = match (
        issue.message.trim().is_empty(),
        issue.code.trim().is_empty(),
    ) {
        (false, false) => format!("{} ({})", issue.message.trim(), issue.code.trim()),
        (false, true) => issue.message.trim().to_owned(),
        (true, false) => issue.code.trim().to_owned(),
        (true, true) => "issue".to_owned(),
    };
    if let Some(retry_at) = issue.retry_at_epoch {
        text.push_str(&format!(
            " · retry {}",
            relative_time_label(now_epoch, retry_at)
        ));
    }
    text
}

pub(crate) fn non_empty_label(label: Option<&String>) -> Option<&str> {
    label
        .map(String::as_str)
        .filter(|label| !label.trim().is_empty())
}

/// Operator-facing freshness age for one account. Pure over an explicit
/// `now` so tests stay deterministic; render passes wall-clock time.
#[must_use]
pub fn freshness_age_label(now_epoch: i64, account: &UsageAccount) -> String {
    if account.freshness_phase == UsageFreshnessPhaseV1::Refreshing {
        return "refreshing…".to_owned();
    }
    let Some(last_good) = account.last_good_at_epoch else {
        return "never updated".to_owned();
    };
    let age_secs = now_epoch.saturating_sub(last_good).max(0);
    let updated = updated_age_label(age_secs);
    if account.is_stale
        || matches!(
            account.freshness_phase,
            UsageFreshnessPhaseV1::Stale | UsageFreshnessPhaseV1::Failed
        )
    {
        format!("stale · {updated}")
    } else {
        updated
    }
}

pub(crate) fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Fallback display name for an unresolved provider id. Mirrors Capsule
/// `provider_display_label`: `Anthropic` (not `Anthropic / Claude`) and `xAI`
/// (not `Grok`); every other well-known id already matches the Capsule
/// spelling, and unknown ids pass through untouched.
pub(crate) fn well_known_provider_name(provider_id: &str) -> String {
    match provider_id.to_ascii_lowercase().as_str() {
        "anthropic" | "claude" => "Anthropic".to_owned(),
        "openai" | "codex" => "OpenAI".to_owned(),
        "opencode" => "OpenCode".to_owned(),
        "kimi" | "moonshot" => "Kimi".to_owned(),
        "grok" | "xai" => "xAI".to_owned(),
        "amp" => "Amp".to_owned(),
        "zai" => "Z.AI".to_owned(),
        "minimax" => "MiniMax".to_owned(),
        other => other.to_owned(),
    }
}
