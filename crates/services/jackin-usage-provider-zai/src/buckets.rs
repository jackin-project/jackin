// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Z.AI` quota bucket views and peak-hour notes.

use chrono::{Datelike, Timelike};
use jackin_protocol::control::{QuotaBucketView, UsageSnapshotStatus};
use jackin_usage_provider_core::{compact_count, epoch_seconds_from_maybe_ms, timed_bucket};

use super::ZaiLimitRaw;

/// Peak-rate window: Mon–Fri 06:00–10:00 UTC burns credits at 1×,
/// everything else at 0.5×. Source: the GLM Coding Plan rate note captured
/// in `ref-contracts-B.md` §2 — no endpoint exposes the rate, so it is
/// derived client-side from the clock and goes stale silently if the plan
/// changes; pace-note only, never quota math.
pub(crate) const ZAI_PEAK_START_HOUR_UTC: u32 = 6;
pub(crate) const ZAI_PEAK_END_HOUR_UTC: u32 = 10;

/// True while the spend clock is inside the peak-rate window.
pub(crate) fn zai_is_peak(now: i64) -> bool {
    let Some(moment) = chrono::DateTime::from_timestamp(now, 0).map(|date| date.naive_utc()) else {
        return false;
    };
    matches!(
        moment.weekday(),
        chrono::Weekday::Mon
            | chrono::Weekday::Tue
            | chrono::Weekday::Wed
            | chrono::Weekday::Thu
            | chrono::Weekday::Fri
    ) && (ZAI_PEAK_START_HOUR_UTC..ZAI_PEAK_END_HOUR_UTC).contains(&moment.hour())
}

pub(crate) fn zai_credit_rate_note(now: i64) -> &'static str {
    if zai_is_peak(now) {
        "peak 1× rate"
    } else {
        "off-peak 0.5× rate"
    }
}

/// Top-two models by `usageDetails` consumption, e.g.
/// `top glm-5 71% · glm-4.5 29%`. `None` when no model breakdown is present.
pub(crate) fn zai_model_note(limit: &ZaiLimitRaw) -> Option<String> {
    let mut details = limit.usage_details.clone();
    details.retain(|detail| {
        detail.usage.is_some_and(|usage| usage > 0)
            && detail
                .model_code
                .as_deref()
                .is_some_and(|code| !code.trim().is_empty())
    });
    if details.is_empty() {
        return None;
    }
    details.sort_by_key(|detail| std::cmp::Reverse(detail.usage.unwrap_or(0)));
    let total: i64 = details.iter().filter_map(|detail| detail.usage).sum();
    let parts = details
        .iter()
        .take(2)
        .map(|detail| {
            let code = detail.model_code.as_deref().unwrap_or_default().trim();
            if total > 0 {
                let used = detail.usage.unwrap_or(0).max(0);
                let share = (i128::from(used) * 100 / i128::from(total)).clamp(0, 100) as i64;
                format!("{code} {share}%")
            } else {
                code.to_owned()
            }
        })
        .collect::<Vec<_>>();
    Some(format!("top {}", parts.join(" · ")))
}

pub fn zai_bucket(label: &str, limit: &ZaiLimitRaw, now: i64) -> QuotaBucketView {
    let used_percent = limit.used_percent();
    let remaining = used_percent.map(|used| 100u8.saturating_sub(used));
    let reset_at = limit.next_reset_time.map(epoch_seconds_from_maybe_ms);
    let mut parts = Vec::new();
    if matches!(label, "MCP" | "Web search") {
        parts.extend(zai_count_line(limit));
    } else if limit.limit_type == "CREDIT_LIMIT" {
        parts.push(zai_credit_rate_note(now).to_owned());
    }
    parts.extend(zai_model_note(limit));
    let detail = (!parts.is_empty()).then(|| parts.join(" · "));
    timed_bucket(
        label,
        limit
            .current_value
            .map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        limit
            .usage
            .map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        remaining,
        reset_at,
        now,
        detail.as_deref(),
        UsageSnapshotStatus::Fresh,
    )
}

pub fn zai_count_line(limit: &ZaiLimitRaw) -> Option<String> {
    let total = limit.usage.filter(|value| *value > 0)?;
    let used = if let Some(remaining) = limit.remaining {
        let from_remaining = total.saturating_sub(remaining);
        limit
            .current_value
            .map_or(from_remaining, |current| from_remaining.max(current))
    } else {
        limit.current_value?
    }
    .clamp(0, total);
    let remaining = total.saturating_sub(used);
    Some(format!(
        "{} / {} ({} remaining)",
        compact_count(u64::try_from(used).unwrap_or(0)),
        compact_count(u64::try_from(total).unwrap_or(0)),
        compact_count(u64::try_from(remaining).unwrap_or(0))
    ))
}
