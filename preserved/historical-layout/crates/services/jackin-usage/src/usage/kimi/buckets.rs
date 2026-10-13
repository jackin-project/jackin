// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Kimi` quota buckets and window views.

use super::super::{
    QuotaBucketView, StatusSlot, UsageSnapshotStatus, compact_count, quota_pace_label,
    timed_bucket, with_status_slot,
};

use super::{
    KimiCount, KimiPool, KimiPools, KimiReset, KimiUsageDetail, KimiUsageResponse, KimiUsages,
    KimiWindow,
};

impl KimiUsageResponse {
    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        // The pools object is the precise per-window meter set; when present
        // it supersedes the coarser `usage` summary and `limits[]` shapes so
        // the same window is never rendered twice.
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
        // rate (short/active) window on top, then Weekly — an operator
        // override of CodexBar's Weekly, Rate Limit order.
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
            kimi_pool_bucket(pool, "5-hour", 5 * 60 * 60, now),
            Some(StatusSlot::Session),
        ));
    }
    if let Some(pool) = &pools.limit_7d {
        buckets.push(with_status_slot(
            kimi_pool_bucket(pool, "Weekly", 7 * 24 * 60 * 60, now),
            Some(StatusSlot::Weekly),
        ));
    }
    if let Some(pool) = &pools.limit_month_total {
        buckets.push(kimi_pool_bucket(pool, "Monthly", 30 * 24 * 60 * 60, now));
    }
    buckets
}

pub(crate) fn kimi_pool_bucket(
    pool: &KimiPool,
    fallback_label: &str,
    window_seconds: i64,
    now: i64,
) -> QuotaBucketView {
    let limit = pool.limit.as_ref().and_then(KimiCount::value);
    let used = pool.used.as_ref().and_then(KimiCount::value).or_else(|| {
        limit.and_then(|limit| {
            pool.remaining
                .as_ref()
                .and_then(KimiCount::value)
                .map(|remaining| limit.saturating_sub(remaining))
        })
    });
    let used_percent = pool.used_percent();
    let remaining = used_percent.map(|used| 100u8.saturating_sub(used));
    let reset_at = pool.reset_time.as_ref().and_then(KimiReset::epoch);
    let pace = quota_pace_label(remaining, reset_at, Some(window_seconds), now);
    let pace = kimi_pace_with_over_cap(pool.used_percent_raw(), pace);
    timed_bucket(
        &pool.label(fallback_label),
        used.map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        limit.map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        remaining,
        reset_at,
        now,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    )
}

/// Prefix the raw over-cap figure ahead of the pace note (`142% used · …`),
/// or pass the pace through when at/below cap.
pub(crate) fn kimi_pace_with_over_cap(
    raw_percent: Option<f64>,
    pace: Option<String>,
) -> Option<String> {
    let over_cap = raw_percent.and_then(kimi_over_cap_label);
    match (over_cap, pace) {
        (Some(over_cap), Some(pace)) => Some(format!("{over_cap} · {pace}")),
        (Some(over_cap), None) => Some(over_cap),
        (None, pace) => pace,
    }
}

impl KimiUsageDetail {
    pub(crate) fn limit_value(&self) -> Option<i64> {
        self.limit.as_ref().and_then(KimiCount::value)
    }

    pub(crate) fn used_value(&self) -> Option<i64> {
        self.used.as_ref().and_then(KimiCount::value)
    }

    pub(crate) fn remaining_value(&self) -> Option<i64> {
        self.remaining.as_ref().and_then(KimiCount::value)
    }

    /// Raw used percent, unclamped (see [`KimiPool::used_percent_raw`]).
    pub(crate) fn used_percent_raw(&self) -> Option<f64> {
        let limit = self.limit_value()?.max(0);
        if limit == 0 {
            return None;
        }
        let used = self.used_value().or_else(|| {
            self.remaining_value()
                .map(|remaining| limit.saturating_sub(remaining))
        })?;
        #[expect(clippy::cast_precision_loss, reason = "count magnitudes fit f64")]
        Some(used.max(0) as f64 / limit as f64 * 100.0)
    }

    pub(crate) fn used_percent(&self) -> Option<u8> {
        self.used_percent_raw().map(|raw| {
            #[expect(
                clippy::cast_sign_loss,
                reason = "raw percent clamped non-negative; clamp bounds the f64→u8 cast"
            )]
            {
                raw.round().clamp(0.0, 100.0) as u8
            }
        })
    }
}

/// Over-cap label preserving the raw provider figure (`142% used`), or `None`
/// at/below cap. Only the bar geometry clamps; the raw overage stays visible
/// (T02; the Muse lane renders the same form).
pub(crate) fn kimi_over_cap_label(raw_percent: f64) -> Option<String> {
    if !raw_percent.is_finite() || raw_percent <= 100.0 {
        return None;
    }
    Some(if raw_percent.fract() == 0.0 {
        format!("{raw_percent:.0}% used")
    } else {
        format!("{raw_percent:.1}% used")
    })
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

pub(crate) fn kimi_bucket(
    label: &str,
    detail: &KimiUsageDetail,
    window: Option<&KimiWindow>,
    now: i64,
) -> QuotaBucketView {
    let limit = detail.limit_value();
    let used = detail.used_value().or_else(|| {
        limit.and_then(|limit| {
            detail
                .remaining_value()
                .map(|remaining| limit.saturating_sub(remaining))
        })
    });
    let used_percent = detail.used_percent();
    let remaining = used_percent.map(|used| 100u8.saturating_sub(used));
    let reset_at = detail.reset_time.as_ref().and_then(KimiReset::epoch);
    let window_seconds = kimi_window_seconds(label, window);
    let pace = quota_pace_label(remaining, reset_at, window_seconds, now);
    let pace = kimi_pace_with_over_cap(detail.used_percent_raw(), pace);
    timed_bucket(
        label,
        used.map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        limit.map(|value| compact_count(u64::try_from(value.max(0)).unwrap_or(0))),
        remaining,
        reset_at,
        now,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    )
}

pub(crate) fn kimi_window_seconds(label: &str, window: Option<&KimiWindow>) -> Option<i64> {
    (label == "Rate Limit")
        .then(|| window.and_then(KimiWindow::seconds))
        .flatten()
}
