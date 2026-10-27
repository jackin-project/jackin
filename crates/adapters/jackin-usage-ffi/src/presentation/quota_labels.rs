// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    CanonicalRemainingPercent, CountQuota, PercentObservationOrigin, ProviderQuotaState,
    QuotaBucketView, RawPercentPoints, UsageMeterSide, UsageSeverity, UsageSnapshotStatus,
};
use jackin_protocol::usage_broker::{UsageLimitWindowV1, UsageMetricGroupV1, UsageMetricValueV1};
use jackin_usage_provider_core::{
    PercentStyle, UsageFormatPrefs, count_observation_detail_label, count_summary_label,
};

fn raw_percent_observation_label(
    side: &str,
    percent: RawPercentPoints,
    origin: Option<PercentObservationOrigin>,
) -> String {
    let qualifier = match origin {
        Some(PercentObservationOrigin::ProviderReported) => "Reported",
        Some(PercentObservationOrigin::Derived) => "Derived",
        Some(PercentObservationOrigin::HistoricalUnknown) | None => "Observed",
    };
    format!("{qualifier} {side}: {}%", percent.display_value())
}

fn count_observation_label(count: CountQuota) -> Option<String> {
    let fields = [
        ("used", count.used),
        ("limit", count.limit),
        ("remaining", count.remaining),
    ]
    .into_iter()
    .filter_map(|(name, value)| value.map(|value| format!("{name} {value}")))
    .collect::<Vec<_>>();
    (!fields.is_empty()).then(|| {
        format!(
            "Reported count ({}/{}): {}",
            count.unit.wire_label(),
            count.period.wire_label(),
            fields.join(", ")
        )
    })
}

fn raw_provider_state_summary(
    provider_state: Option<ProviderQuotaState>,
    used_percent: Option<RawPercentPoints>,
    used_percent_origin: Option<PercentObservationOrigin>,
    remaining_percent: Option<RawPercentPoints>,
    remaining_percent_origin: Option<PercentObservationOrigin>,
    count_quota: Option<CountQuota>,
    money_used_percent: Option<RawPercentPoints>,
) -> Option<String> {
    let count_label = count_quota.and_then(count_observation_label);
    raw_provider_state_summary_with_count_label(
        provider_state,
        used_percent,
        used_percent_origin,
        remaining_percent,
        remaining_percent_origin,
        count_label.as_deref(),
        money_used_percent,
    )
}

fn raw_provider_state_summary_with_count_label(
    provider_state: Option<ProviderQuotaState>,
    used_percent: Option<RawPercentPoints>,
    used_percent_origin: Option<PercentObservationOrigin>,
    remaining_percent: Option<RawPercentPoints>,
    remaining_percent_origin: Option<PercentObservationOrigin>,
    count_label: Option<&str>,
    money_used_percent: Option<RawPercentPoints>,
) -> Option<String> {
    let state = provider_state?;
    let mut parts = vec![jackin_usage_provider_core::provider_quota_state_label(state).to_owned()];
    if let Some(used) = used_percent {
        parts.push(raw_percent_observation_label(
            "used",
            used,
            used_percent_origin,
        ));
    }
    if let Some(remaining) = remaining_percent {
        parts.push(raw_percent_observation_label(
            "remaining",
            remaining,
            remaining_percent_origin,
        ));
    }
    if let Some(count) = count_label {
        parts.push(count.to_owned());
    }
    if let Some(money) = money_used_percent {
        parts.push(format!("Money ratio: {}% used", money.display_value()));
    }
    Some(parts.join(" · "))
}

fn metric_window_provider_state_summary(group: &UsageMetricGroupV1) -> Option<String> {
    let UsageMetricValueV1::Window {
        used_percent,
        used_percent_origin,
        remaining_percent,
        remaining_percent_origin,
        count_quota,
        provider_quota_state,
        money_used_percent,
        ..
    } = &group.value
    else {
        return None;
    };
    raw_provider_state_summary(
        *provider_quota_state,
        *used_percent,
        *used_percent_origin,
        *remaining_percent,
        *remaining_percent_origin,
        *count_quota,
        *money_used_percent,
    )
}

fn window_provider_state_summary(window: &UsageLimitWindowV1) -> Option<String> {
    raw_provider_state_summary(
        window.provider_quota_state,
        window.used_percent,
        window.used_percent_origin,
        window.remaining_percent,
        window.remaining_percent_origin,
        window.count_quota,
        window.money_used_percent,
    )
}

fn raw_source_money_collision_label(
    used_percent: Option<RawPercentPoints>,
    used_percent_origin: Option<PercentObservationOrigin>,
    remaining_percent: Option<RawPercentPoints>,
    remaining_percent_origin: Option<PercentObservationOrigin>,
    money_used_percent: Option<RawPercentPoints>,
) -> Option<String> {
    let money = money_used_percent?;
    let mut parts = Vec::new();
    if let Some(used) = used_percent {
        parts.push(raw_percent_observation_label(
            "used",
            used,
            used_percent_origin,
        ));
    }
    if let Some(remaining) = remaining_percent {
        parts.push(raw_percent_observation_label(
            "remaining",
            remaining,
            remaining_percent_origin,
        ));
    }
    if parts.is_empty() {
        return None;
    }
    parts.push(format!("Money ratio: {}% used", money.display_value()));
    Some(parts.join(" · "))
}

pub(super) fn metric_window_percent_label(
    group: &UsageMetricGroupV1,
    style: PercentStyle,
) -> Option<String> {
    let UsageMetricValueV1::Window {
        remaining_percent,
        remaining_percent_origin,
        used_percent,
        used_percent_origin,
        count_quota,
        provider_quota_state: _,
        money_used_percent,
        meter_percent,
        meter_side,
        ..
    } = &group.value
    else {
        return None;
    };
    if let Some(summary) = metric_window_provider_state_summary(group) {
        return Some(summary);
    }
    if let Some(summary) = raw_source_money_collision_label(
        *used_percent,
        *used_percent_origin,
        *remaining_percent,
        *remaining_percent_origin,
        *money_used_percent,
    ) {
        return Some(summary);
    }
    if let Some(count) = count_quota
        && let Some(label) = count_summary_label(&count_bucket(*count))
    {
        return Some(label);
    }
    if let Some(value) = used_percent {
        return Some(format!("{}% used", value.display_value()));
    }
    if let Some(value) = remaining_percent {
        if style == PercentStyle::Used
            && let Ok(canonical) = CanonicalRemainingPercent::new(value.get())
        {
            return Some(jackin_usage_provider_core::percent_headline(
                canonical,
                UsageFormatPrefs {
                    percent_style: style,
                    ..UsageFormatPrefs::default()
                },
            ));
        }
        return Some(format!("{}% left", value.display_value()));
    }
    if let Some(value) = money_used_percent {
        return Some(format!("{}% used", value.display_value()));
    }
    match (meter_percent, meter_side) {
        (Some(value), Some(UsageMeterSide::Used)) => Some(format!(
            "{}% used",
            jackin_usage_provider_core::format_percent_points(value.get())
        )),
        (Some(value), Some(UsageMeterSide::Remaining)) => Some(format!(
            "{}% left",
            jackin_usage_provider_core::format_percent_points(value.get())
        )),
        _ => None,
    }
}

pub(super) fn metric_window_bar_label(
    group: &UsageMetricGroupV1,
    style: PercentStyle,
) -> Option<String> {
    let label = metric_window_percent_label(group, style)?;
    if style != PercentStyle::Used || metric_window_provider_state_summary(group).is_some() {
        return Some(label);
    }
    let UsageMetricValueV1::Window {
        used_percent,
        used_percent_origin,
        remaining_percent,
        remaining_percent_origin,
        count_quota,
        money_used_percent,
        ..
    } = &group.value
    else {
        return Some(label);
    };
    if count_quota
        .and_then(|count| count_summary_label(&count_bucket(count)))
        .is_some()
        || raw_source_money_collision_label(
            *used_percent,
            *used_percent_origin,
            *remaining_percent,
            *remaining_percent_origin,
            *money_used_percent,
        )
        .is_some()
    {
        return Some(label);
    }
    Some(label.strip_suffix(" used").unwrap_or(&label).to_owned())
}

pub(super) fn metric_window_headline(
    group: &UsageMetricGroupV1,
    format_prefs: UsageFormatPrefs,
) -> Option<String> {
    let value = metric_window_percent_label(group, format_prefs.percent_style)?;
    if metric_window_provider_state_summary(group).is_some() {
        return Some(value);
    }
    Some(
        group
            .scope
            .model
            .as_ref()
            .map_or_else(|| value.clone(), |model| format!("{model} {value}")),
    )
}

pub(super) fn window_headline(
    window: &UsageLimitWindowV1,
    format_prefs: UsageFormatPrefs,
) -> String {
    window_numeric_label(window, format_prefs).unwrap_or_else(|| window.value_label.clone())
}

pub(super) fn window_numeric_label(
    window: &UsageLimitWindowV1,
    format_prefs: UsageFormatPrefs,
) -> Option<String> {
    if let Some(summary) = window_provider_state_summary(window) {
        return Some(summary);
    }
    if let Some(summary) = raw_source_money_collision_label(
        window.used_percent,
        window.used_percent_origin,
        window.remaining_percent,
        window.remaining_percent_origin,
        window.money_used_percent,
    ) {
        return Some(summary);
    }
    if let Some(count) = window.count_quota
        && let Some(label) = count_summary_label(&count_bucket(count))
    {
        return Some(label);
    }
    if let Some(used) = window.used_percent {
        return Some(format!("{}% used", used.display_value()));
    }
    if let Some(remaining) = window.remaining_percent {
        if format_prefs.percent_style == PercentStyle::Used
            && let Ok(canonical) = CanonicalRemainingPercent::new(remaining.get())
        {
            return Some(jackin_usage_provider_core::percent_headline(
                canonical,
                format_prefs,
            ));
        }
        return Some(format!("{}% left", remaining.display_value()));
    }
    if let Some(money) = window.money_used_percent {
        return Some(format!("{}% used", money.display_value()));
    }
    match (window.meter_percent, window.meter_side) {
        (Some(value), Some(UsageMeterSide::Used)) => Some(format!(
            "{}% used",
            jackin_usage_provider_core::format_percent_points(value.get())
        )),
        (Some(value), Some(UsageMeterSide::Remaining)) => Some(format!(
            "{}% left",
            jackin_usage_provider_core::format_percent_points(value.get())
        )),
        _ => None,
    }
}

pub(super) fn window_display_summary(
    window: &UsageLimitWindowV1,
    format_prefs: UsageFormatPrefs,
) -> String {
    if window_provider_state_summary(window).is_some()
        || (window.money_used_percent.is_some()
            && (window.used_percent.is_some() || window.remaining_percent.is_some()))
    {
        return window_numeric_label(window, format_prefs)
            .unwrap_or_else(|| window.value_label.clone());
    }
    window.value_label.clone()
}

pub(super) fn window_detail_summary(
    window: &UsageLimitWindowV1,
    format_prefs: UsageFormatPrefs,
) -> String {
    let Some(count) = window.count_quota else {
        return window_display_summary(window, format_prefs);
    };

    let detail_count = count_observation_detail_label(count);
    if let Some(summary) = raw_provider_state_summary_with_count_label(
        window.provider_quota_state,
        window.used_percent,
        window.used_percent_origin,
        window.remaining_percent,
        window.remaining_percent_origin,
        Some(&detail_count),
        window.money_used_percent,
    ) {
        return summary;
    }
    if let Some(summary) = raw_source_money_collision_label(
        window.used_percent,
        window.used_percent_origin,
        window.remaining_percent,
        window.remaining_percent_origin,
        window.money_used_percent,
    ) {
        return format!("{summary} · {detail_count}");
    }

    let summary = window_display_summary(window, format_prefs);
    let compact_count = count_summary_label(&count_bucket(count));
    if summary.is_empty() || compact_count.as_deref() == Some(summary.as_str()) {
        detail_count
    } else {
        format!("{summary} · {detail_count}")
    }
}

fn count_bucket(count: CountQuota) -> QuotaBucketView {
    QuotaBucketView {
        label: "quota".to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota: Some(count),
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}
