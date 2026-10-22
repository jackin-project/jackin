// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    CanonicalRemainingPercent, CanonicalUsedPercent, CountQuota, CountQuotaPeriod,
    CountQuotaProvenance, CountQuotaUnit, PercentObservationOrigin, QuotaBucketView, StatusSlot,
    UsageSnapshotStatus,
};
use jackin_usage_provider_core::{
    compact_count, quota_pace_label, timed_bucket, with_status_slot, with_used_percent,
};

use super::{
    KimiCount, KimiPool, KimiPools, KimiReset, KimiUsageDetail, KimiUsageResponse, KimiUsages,
    KimiWindow,
};

impl KimiUsageResponse {
    pub fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        if let Some(KimiUsages::Pools(pools)) = &self.usages
            && pools.any()
        {
            return kimi_pool_buckets(pools, now);
        }
        let (detail, limits) = if let Some(detail) = &self.usage {
            (detail, self.limits.as_slice())
        } else if let Some(KimiUsages::List(usages)) = &self.usages
            && let Some(usage) = usages
                .iter()
                .find(|usage| usage.scope.as_deref() == Some("FEATURE_CODING"))
                .or_else(|| usages.first())
        {
            (&usage.detail, usage.limits.as_slice())
        } else {
            return Vec::new();
        };
        let mut buckets = Vec::new();
        if let Some(rate_limit) = limits.first() {
            buckets.push(with_status_slot(
                kimi_bucket(
                    "Rate Limit",
                    &rate_limit.detail,
                    rate_limit.window.as_ref(),
                    now,
                ),
                Some(StatusSlot::Session),
            ));
        }
        buckets.push(with_status_slot(
            kimi_bucket("Weekly", detail, None, now),
            Some(StatusSlot::Weekly),
        ));
        buckets
    }
}

pub(crate) fn kimi_pool_buckets(pools: &KimiPools, now: i64) -> Vec<QuotaBucketView> {
    let mut buckets = Vec::new();
    if let Some(pool) = &pools.limit_5h {
        buckets.push(with_status_slot(
            parse_kimi_pool_bucket(pool, "5-hour", 5 * 60 * 60, now),
            Some(StatusSlot::Session),
        ));
    }
    if let Some(pool) = &pools.limit_7d {
        buckets.push(with_status_slot(
            parse_kimi_pool_bucket(pool, "Weekly", 7 * 24 * 60 * 60, now),
            Some(StatusSlot::Weekly),
        ));
    }
    if let Some(pool) = &pools.limit_month_total {
        buckets.push(parse_kimi_pool_bucket(
            pool,
            "Monthly",
            30 * 24 * 60 * 60,
            now,
        ));
    }
    buckets
}

fn parse_kimi_pool_bucket(
    raw: &serde_json::Value,
    fallback_label: &str,
    window_seconds: i64,
    now: i64,
) -> QuotaBucketView {
    if let Ok(pool) = serde_json::from_value::<KimiPool>(raw.clone()) {
        kimi_pool_bucket(&pool, fallback_label, window_seconds, now)
    } else {
        let message = format!("Kimi quota window {fallback_label}: malformed pool");
        timed_bucket(
            fallback_label,
            None,
            None,
            None,
            None,
            now,
            Some(&message),
            UsageSnapshotStatus::Unavailable,
        )
    }
}

fn parsed_count(count: Option<&KimiCount>) -> Result<Option<u64>, &'static str> {
    count.map(KimiCount::observation).transpose()
}

fn first_count_error(
    label: &str,
    limit: Option<&KimiCount>,
    used: Option<&KimiCount>,
    remaining: Option<&KimiCount>,
) -> Option<String> {
    [("limit", limit), ("used", used), ("remaining", remaining)]
        .into_iter()
        .find_map(|(field, count)| {
            count
                .and_then(|count| count.observation().err())
                .map(|error| format!("Kimi quota window {label}: {field}: {error}"))
        })
}

fn count_quota(
    limit: Option<u64>,
    used: Option<u64>,
    remaining: Option<u64>,
) -> Option<CountQuota> {
    (limit.is_some() || used.is_some() || remaining.is_some()).then(|| {
        CountQuota::new(
            used,
            limit,
            remaining,
            CountQuotaUnit::Unknown,
            CountQuotaPeriod::Unknown,
            CountQuotaProvenance::ProviderReported,
        )
    })
}

struct KimiBucketObservations<'a> {
    label: &'a str,
    limit_raw: Option<&'a KimiCount>,
    used_raw: Option<&'a KimiCount>,
    remaining_raw: Option<&'a KimiCount>,
    used_percent: Result<Option<CanonicalUsedPercent>, ()>,
    reset_at: Option<i64>,
    window_seconds: Option<i64>,
    now: i64,
}

fn kimi_bucket_from_observations(observations: KimiBucketObservations<'_>) -> QuotaBucketView {
    let KimiBucketObservations {
        label,
        limit_raw,
        used_raw,
        remaining_raw,
        used_percent,
        reset_at,
        window_seconds,
        now,
    } = observations;
    let limit = parsed_count(limit_raw).ok().flatten();
    let used = parsed_count(used_raw).ok().flatten();
    let remaining = parsed_count(remaining_raw).ok().flatten();
    let count_quota = count_quota(limit, used, remaining);
    let invalid_used_percent = used_percent.is_err();
    let used_percent = used_percent.ok().flatten();

    let diagnostic = first_count_error(label, limit_raw, used_raw, remaining_raw).or_else(|| {
        invalid_used_percent
            .then(|| format!("Kimi quota window {label}: invalid reported used ratio"))
    });
    let quantitative = count_quota.is_some() || used_percent.is_some();
    let status = if quantitative {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Unavailable
    };

    let pace = if let Some(diagnostic) = diagnostic {
        Some(diagnostic)
    } else if let Some(used_percent) = used_percent {
        let remaining_points = CanonicalRemainingPercent::new(used_percent.remaining_points()).ok();
        let pace = quota_pace_label(remaining_points, reset_at, window_seconds, now);
        kimi_pace_with_over_cap(used_percent, pace)
    } else if !quantitative {
        Some(format!(
            "Kimi quota window {label}: no quantitative observation"
        ))
    } else {
        None
    };
    let mut view = timed_bucket(
        label,
        used.map(compact_count),
        limit.map(compact_count),
        None,
        reset_at,
        now,
        pace.as_deref(),
        status,
    );
    view.count_quota = count_quota;
    with_used_percent(
        view,
        used_percent.map(|percent| (percent, PercentObservationOrigin::ProviderReported)),
    )
}

pub(crate) fn kimi_pool_bucket(
    pool: &KimiPool,
    fallback_label: &str,
    window_seconds: i64,
    now: i64,
) -> QuotaBucketView {
    let label = pool.label(fallback_label);
    let reset_at = pool.reset_time.as_ref().and_then(KimiReset::epoch);
    kimi_bucket_from_observations(KimiBucketObservations {
        label: &label,
        limit_raw: pool.limit.as_ref(),
        used_raw: pool.used.as_ref(),
        remaining_raw: pool.remaining.as_ref(),
        used_percent: pool.used_percent(),
        reset_at,
        window_seconds: Some(window_seconds),
        now,
    })
}

fn kimi_pace_with_over_cap(
    used_percent: CanonicalUsedPercent,
    pace: Option<String>,
) -> Option<String> {
    let over_cap = kimi_over_cap_label(used_percent.get());
    match (over_cap, pace) {
        (Some(over_cap), Some(pace)) => Some(format!("{over_cap} · {pace}")),
        (Some(over_cap), None) => Some(over_cap),
        (None, pace) => pace,
    }
}

impl KimiUsageDetail {
    pub(crate) fn limit_value(&self) -> Option<u64> {
        self.limit.as_ref().and_then(KimiCount::value)
    }

    pub(crate) fn used_value(&self) -> Option<u64> {
        self.used.as_ref().and_then(KimiCount::value)
    }

    pub(crate) fn remaining_value(&self) -> Option<u64> {
        self.remaining.as_ref().and_then(KimiCount::value)
    }
}

pub(crate) fn kimi_over_cap_label(used_percent: f64) -> Option<String> {
    let used = CanonicalUsedPercent::new(used_percent).ok()?;
    (used.get() > 100.0).then(|| used.label())
}

impl KimiWindow {
    pub(crate) fn seconds(&self) -> Option<i64> {
        let duration = self.duration?;
        let unit = self
            .time_unit
            .as_deref()
            .unwrap_or("hour")
            .to_ascii_lowercase();
        if unit.contains("second") {
            Some(duration)
        } else if unit.contains("minute") {
            Some(duration * 60)
        } else if unit.contains("hour") {
            Some(duration * 60 * 60)
        } else if unit.contains("day") {
            Some(duration * 24 * 60 * 60)
        } else if unit.contains("week") {
            Some(duration * 7 * 24 * 60 * 60)
        } else {
            None
        }
    }
}

pub fn kimi_bucket(
    label: &str,
    detail: &KimiUsageDetail,
    window: Option<&KimiWindow>,
    now: i64,
) -> QuotaBucketView {
    let reset_at = detail.reset_time.as_ref().and_then(KimiReset::epoch);
    let window_seconds = kimi_window_seconds(label, window);
    kimi_bucket_from_observations(KimiBucketObservations {
        label,
        limit_raw: detail.limit.as_ref(),
        used_raw: detail.used.as_ref(),
        remaining_raw: detail.remaining.as_ref(),
        used_percent: Ok(None),
        reset_at,
        window_seconds,
        now,
    })
}

pub fn kimi_window_seconds(label: &str, window: Option<&KimiWindow>) -> Option<i64> {
    (label == "Rate Limit")
        .then(|| window.and_then(KimiWindow::seconds))
        .flatten()
}
