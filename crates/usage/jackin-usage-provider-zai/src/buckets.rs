// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use chrono::{Datelike, Timelike};
use jackin_protocol::control::{
    CanonicalUsedPercent, CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
    PercentObservationOrigin, QuotaBucketView, UsageSnapshotStatus,
};
use jackin_usage_provider_core::{
    compact_count, epoch_seconds_from_maybe_ms, timed_bucket, with_used_percent,
};

use super::ZaiLimitRaw;

pub(crate) const ZAI_PEAK_START_HOUR_UTC: u32 = 6;
pub(crate) const ZAI_PEAK_END_HOUR_UTC: u32 = 10;

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

pub(crate) fn zai_model_note(limit: &ZaiLimitRaw) -> Option<String> {
    let mut details = limit
        .usage_details
        .iter()
        .filter_map(|detail| {
            let usage = zai_count(detail.usage.as_ref())?;
            let code = detail
                .model_code
                .as_deref()
                .filter(|code| !code.trim().is_empty())?;
            (usage > 0).then_some((detail, usage, code.trim()))
        })
        .collect::<Vec<_>>();
    if details.is_empty() {
        return None;
    }
    details.sort_by_key(|(_, usage, _)| std::cmp::Reverse(*usage));
    let total: u128 = details.iter().map(|(_, usage, _)| u128::from(*usage)).sum();
    let parts = details
        .iter()
        .take(2)
        .map(|(_, usage, code)| {
            let Some(share) = (u128::from(*usage) * 100).checked_div(total) else {
                return code.to_string();
            };
            format!("{code} {}%", share.min(100))
        })
        .collect::<Vec<_>>();
    Some(format!("top {}", parts.join(" · ")))
}

pub fn zai_bucket(label: &str, limit: &ZaiLimitRaw, now: i64) -> QuotaBucketView {
    let used_percent = limit.used_percent();
    let used_percent = used_percent.and_then(|points| CanonicalUsedPercent::new(points).ok());
    let used_count = zai_count(limit.current_value.as_ref());
    let limit_count = zai_count(limit.usage.as_ref());
    let remaining_count = zai_count(limit.remaining.as_ref());
    let reset_at = limit.next_reset_time.map(epoch_seconds_from_maybe_ms);
    let mut parts = Vec::new();
    if matches!(label, "MCP" | "Web search") {
        parts.extend(zai_count_line(limit));
    } else if limit.limit_type == "CREDIT_LIMIT" {
        parts.push(zai_credit_rate_note(now).to_owned());
    }
    parts.extend(zai_model_note(limit));
    let invalid_count_fields = zai_invalid_count_fields(limit);
    if !invalid_count_fields.is_empty() {
        parts.push(format!(
            "invalid count field{}: {}",
            if invalid_count_fields.len() == 1 {
                ""
            } else {
                "s"
            },
            invalid_count_fields.join(", ")
        ));
    }
    let detail = (!parts.is_empty()).then(|| parts.join(" · "));
    let mut view = timed_bucket(
        label,
        used_count.map(compact_count),
        limit_count.map(compact_count),
        None,
        reset_at,
        now,
        detail.as_deref(),
        if used_count.is_some()
            || limit_count.is_some()
            || remaining_count.is_some()
            || used_percent.is_some()
        {
            UsageSnapshotStatus::Fresh
        } else {
            UsageSnapshotStatus::Unavailable
        },
    );
    if used_count.is_some() || limit_count.is_some() || remaining_count.is_some() {
        view.count_quota = Some(CountQuota::new(
            used_count,
            limit_count,
            remaining_count,
            CountQuotaUnit::Unknown,
            CountQuotaPeriod::Unknown,
            CountQuotaProvenance::ProviderReported,
        ));
    }
    with_used_percent(
        view,
        used_percent.map(|percent| (percent, PercentObservationOrigin::ProviderReported)),
    )
}

pub fn zai_count_line(limit: &ZaiLimitRaw) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(used) = zai_count(limit.current_value.as_ref()) {
        parts.push(format!("{} used", compact_count(used)));
    }
    if let Some(total) = zai_count(limit.usage.as_ref()) {
        parts.push(format!("{} limit", compact_count(total)));
    }
    if let Some(remaining) = zai_count(limit.remaining.as_ref()) {
        parts.push(format!("{} remaining", compact_count(remaining)));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn zai_count(value: Option<&serde_json::Value>) -> Option<u64> {
    let value = value?;
    value.as_u64().or_else(|| {
        value
            .as_str()
            .and_then(|text| text.trim().parse::<u64>().ok())
    })
}

fn zai_invalid_count_fields(limit: &ZaiLimitRaw) -> Vec<&'static str> {
    [
        ("currentValue", limit.current_value.as_ref()),
        ("usage", limit.usage.as_ref()),
        ("remaining", limit.remaining.as_ref()),
    ]
    .into_iter()
    .filter_map(|(name, value)| (value.is_some() && zai_count(value).is_none()).then_some(name))
    .collect()
}
