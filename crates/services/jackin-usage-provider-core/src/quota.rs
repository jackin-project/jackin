// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    CanonicalRemainingPercent, CountQuota, CountQuotaUnit, MeterPercentPoints,
    PercentObservationOrigin, ProviderQuotaState, QuotaBucketView, RawPercentPoints,
    UsageMeterSide,
};

#[must_use]
pub fn select_quota_meter(
    bucket: &QuotaBucketView,
) -> (Option<MeterPercentPoints>, Option<UsageMeterSide>) {
    match bucket.provider_quota_state {
        Some(ProviderQuotaState::Exhausted) => {
            return (
                Some(MeterPercentPoints::ZERO),
                Some(UsageMeterSide::Remaining),
            );
        }
        Some(ProviderQuotaState::Unlimited) => return (None, None),
        None => {}
    }

    if let Some(count_quota) = bucket.count_quota {
        let geometry = match count_quota.unit {
            CountQuotaUnit::Requests => count_quota.remaining_meter_percent(),
            CountQuotaUnit::Unknown => provider_reported_remaining(bucket),
        };
        return (geometry, geometry.map(|_| UsageMeterSide::Remaining));
    }

    if bucket.used_percent_origin == Some(PercentObservationOrigin::ProviderReported)
        && let Some(used) = bucket.used_percent
    {
        return (Some(used.meter_geometry()), Some(UsageMeterSide::Used));
    }
    if let Some(remaining) = provider_reported_remaining(bucket) {
        return (Some(remaining), Some(UsageMeterSide::Remaining));
    }
    if let Some(money_used) = money_used_percent(bucket) {
        return (
            Some(money_used.meter_geometry()),
            Some(UsageMeterSide::Used),
        );
    }
    if let Some(used) = bucket.used_percent {
        return (Some(used.meter_geometry()), Some(UsageMeterSide::Used));
    }
    if let Some(remaining) = bucket.remaining_percent {
        return (
            Some(remaining.meter_geometry()),
            Some(UsageMeterSide::Remaining),
        );
    }
    (None, None)
}

#[must_use]
pub fn quota_bucket_remaining_rank_points(bucket: &QuotaBucketView) -> Option<f64> {
    let raw_used = bucket.used_percent.map(RawPercentPoints::from);
    let raw_remaining = bucket.remaining_percent.map(RawPercentPoints::from);
    quota_source_remaining_rank_points(
        bucket.count_quota,
        raw_used,
        bucket.used_percent_origin,
        raw_remaining,
        bucket.remaining_percent_origin,
        money_used_percent(bucket),
        bucket.provider_quota_state,
    )
}

#[must_use]
pub fn quota_bucket_has_observation(bucket: &QuotaBucketView) -> bool {
    bucket.provider_quota_state.is_some()
        || bucket.count_quota.is_some_and(|count| {
            count.used.is_some() || count.limit.is_some() || count.remaining.is_some()
        })
        || bucket.used_percent.is_some()
        || bucket.remaining_percent.is_some()
        || bucket.used_money.is_some()
        || bucket.limit_money.is_some()
}

#[must_use]
pub fn quota_source_remaining_rank_points(
    count_quota: Option<CountQuota>,
    used_percent: Option<RawPercentPoints>,
    used_percent_origin: Option<PercentObservationOrigin>,
    remaining_percent: Option<RawPercentPoints>,
    remaining_percent_origin: Option<PercentObservationOrigin>,
    money_used_percent: Option<RawPercentPoints>,
    provider_quota_state: Option<ProviderQuotaState>,
) -> Option<f64> {
    match provider_quota_state {
        Some(ProviderQuotaState::Exhausted) => return Some(0.0),
        Some(ProviderQuotaState::Unlimited) => return None,
        None => {}
    }

    if let Some(count) = count_quota {
        return match count.unit {
            CountQuotaUnit::Requests => requests_count_remaining_rank_points(count),
            CountQuotaUnit::Unknown => {
                provider_reported_raw_remaining(remaining_percent, remaining_percent_origin)
                    .map(RawPercentPoints::get)
            }
        };
    }

    if used_percent_origin == Some(PercentObservationOrigin::ProviderReported)
        && let Some(used) = used_percent
    {
        return Some(100.0 - used.get());
    }
    if let Some(remaining) =
        provider_reported_raw_remaining(remaining_percent, remaining_percent_origin)
    {
        return Some(remaining.get());
    }
    if let Some(money_used) = money_used_percent {
        return Some(100.0 - money_used.get());
    }
    if let Some(used) = used_percent {
        return Some(100.0 - used.get());
    }
    remaining_percent.map(RawPercentPoints::get)
}

fn requests_count_remaining_rank_points(count: CountQuota) -> Option<f64> {
    let limit = count.limit.filter(|limit| *limit > 0)?;
    let remaining = count.remaining?;
    let scaled = u128::from(remaining).checked_mul(100)?;
    Some(scaled as f64 / u128::from(limit) as f64)
}

#[must_use]
pub fn money_used_percent(bucket: &QuotaBucketView) -> Option<RawPercentPoints> {
    let used = bucket.used_money.as_ref()?;
    let limit = bucket.limit_money.as_ref()?;
    let points = used.raw_percent_points_of(limit)?;
    RawPercentPoints::new(points).ok()
}

fn provider_reported_remaining(bucket: &QuotaBucketView) -> Option<MeterPercentPoints> {
    (bucket.remaining_percent_origin == Some(PercentObservationOrigin::ProviderReported))
        .then_some(bucket.remaining_percent)
        .flatten()
        .map(CanonicalRemainingPercent::meter_geometry)
}

fn provider_reported_raw_remaining(
    remaining_percent: Option<RawPercentPoints>,
    origin: Option<PercentObservationOrigin>,
) -> Option<RawPercentPoints> {
    (origin == Some(PercentObservationOrigin::ProviderReported))
        .then_some(remaining_percent)
        .flatten()
}

#[must_use]
pub const fn provider_quota_state_label(state: ProviderQuotaState) -> &'static str {
    match state {
        ProviderQuotaState::Exhausted => "Exhausted",
        ProviderQuotaState::Unlimited => "Unlimited",
    }
}

#[must_use]
pub fn provider_quota_state_summary(bucket: &QuotaBucketView) -> Option<String> {
    let state = bucket.provider_quota_state?;
    let mut parts = vec![provider_quota_state_label(state).to_owned()];

    if let Some(used) = bucket.used_percent {
        parts.push(source_percent_label(
            "used",
            used.get(),
            bucket.used_percent_origin,
        ));
    }
    if let Some(remaining) = bucket.remaining_percent {
        parts.push(source_percent_label(
            "remaining",
            remaining.get(),
            bucket.remaining_percent_origin,
        ));
    }
    if let Some(count) = crate::count_summary_label(bucket) {
        parts.push(count);
    }
    if let Some(money_used) = money_used_percent(bucket) {
        parts.push(format!("{}% used", money_used.display_value()));
    }
    let label_prefix = if bucket.status_slot == Some(jackin_protocol::control::StatusSlot::Spend) {
        "Monthly cap"
    } else if bucket.used_money.is_some() || bucket.limit_money.is_some() {
        "Budget"
    } else {
        "Values"
    };
    match (&bucket.used_label, &bucket.limit_label) {
        (Some(used), Some(limit)) => parts.push(format!("{label_prefix}: {used} / {limit}")),
        (Some(label), None) | (None, Some(label)) => parts.push(label.clone()),
        (None, None) => {}
    }

    Some(parts.join(" · "))
}

fn source_percent_label(
    side: &str,
    points: f64,
    origin: Option<PercentObservationOrigin>,
) -> String {
    let qualifier = match origin {
        Some(PercentObservationOrigin::ProviderReported) => "Reported ",
        Some(PercentObservationOrigin::Derived) => "Derived ",
        Some(PercentObservationOrigin::HistoricalUnknown) | None => "Observed ",
    };
    format!(
        "{qualifier}{side}: {}%",
        crate::format_percent_points(points)
    )
}

#[cfg(test)]
#[path = "quota/numeric_tests.rs"]
mod tests;
