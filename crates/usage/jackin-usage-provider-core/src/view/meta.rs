// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    CanonicalRemainingPercent, CanonicalUsedPercent, PercentObservationOrigin,
};

use super::super::{
    FocusedUsageView, QuotaBucketView, StatusSlot, UsageSeverity, UsageSnapshotStatus, UsageSource,
    reset_label,
};
use super::{count_summary_label, summary_bucket};

pub fn usage_tab_source_label(view: &FocusedUsageView) -> String {
    let freshness = match view.status {
        UsageSnapshotStatus::Fresh => "fresh",
        UsageSnapshotStatus::Stale => "stale",
        UsageSnapshotStatus::NeedsLogin => "needs login",
        UsageSnapshotStatus::NeedsSecret => "needs secret",
        UsageSnapshotStatus::Unsupported => "unsupported",
        UsageSnapshotStatus::Unavailable => "unavailable",
        UsageSnapshotStatus::Error => "error",
    };
    let source = match view.source {
        UsageSource::ProviderApi => "provider",
        UsageSource::Cli => "managed CLI",
        UsageSource::LocalLogs => "local estimate",
        UsageSource::Cache => "cache",
        UsageSource::None => "no source",
    };
    format!("{freshness} · {source}")
}

pub fn usage_tab_status_label(view: &FocusedUsageView) -> String {
    if view.status == UsageSnapshotStatus::Fresh
        && let Some(bucket) = summary_bucket(&view.buckets)
        && super::super::quota_bucket_has_observation(bucket)
    {
        if let Some(summary) = super::super::provider_quota_state_summary(bucket) {
            return summary;
        }
        let mut label = String::new();
        if bucket.status_slot.is_none() && !bucket.label.is_empty() {
            label.push_str(&bucket.label);
            label.push(' ');
        }
        let mut values = Vec::new();
        if let Some(used) = bucket.used_percent {
            values.push(used.label());
        }
        if let Some(remaining) = bucket.remaining_percent {
            values.push(remaining.label());
        }
        if let Some(count) = count_summary_label(bucket) {
            values.push(count);
        }
        if values.is_empty() {
            match (&bucket.used_label, &bucket.limit_label) {
                (Some(used), Some(limit)) => values.push(format!("{used} of {limit}")),
                (Some(used), None) | (None, Some(used)) => values.push(used.clone()),
                (None, None) => match (&bucket.used_money, &bucket.limit_money) {
                    (Some(used), Some(limit)) => values.push(format!(
                        "{} of {}",
                        used.format_compact(),
                        limit.format_compact()
                    )),
                    (Some(used), None) => values.push(used.format_compact()),
                    (None, Some(limit)) => values.push(limit.format_compact()),
                    (None, None) => {}
                },
            }
        }
        label.push_str(&values.join(" · "));
        if let Some(reset) = &bucket.reset_label {
            label.push_str(" · ");
            label.push_str(reset);
        }
        return label;
    }
    match view.status {
        UsageSnapshotStatus::Fresh => "fresh".to_owned(),
        UsageSnapshotStatus::Stale => "stale".to_owned(),
        UsageSnapshotStatus::NeedsLogin => "needs login".to_owned(),
        UsageSnapshotStatus::NeedsSecret => "needs secret".to_owned(),
        UsageSnapshotStatus::Unsupported => "unsupported".to_owned(),
        UsageSnapshotStatus::Unavailable => "unavailable".to_owned(),
        UsageSnapshotStatus::Error => "error".to_owned(),
    }
}

pub fn bucket(
    label: &str,
    used_label: Option<String>,
    limit_label: Option<String>,
    remaining_percent: Option<(CanonicalRemainingPercent, PercentObservationOrigin)>,
    reset_label: Option<String>,
    pace_label: Option<&str>,
    status: UsageSnapshotStatus,
) -> QuotaBucketView {
    let (remaining_percent, remaining_percent_origin) = remaining_percent
        .map_or((None, None), |(percent, origin)| {
            (Some(percent), Some(origin))
        });
    QuotaBucketView {
        label: label.to_owned(),
        used_label,
        limit_label,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent,
        remaining_percent_origin,
        count_quota: None,
        provider_quota_state: None,
        reset_label,
        resets_at: None,
        status_slot: None,
        pace_label: pace_label.map(str::to_owned),
        status,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
    }
}

#[must_use]
pub fn with_used_percent(
    mut view: QuotaBucketView,
    used_percent: Option<(CanonicalUsedPercent, PercentObservationOrigin)>,
) -> QuotaBucketView {
    (view.used_percent, view.used_percent_origin) = used_percent
        .map_or((None, None), |(percent, origin)| {
            (Some(percent), Some(origin))
        });
    view
}

pub fn with_status_slot(mut view: QuotaBucketView, slot: Option<StatusSlot>) -> QuotaBucketView {
    view.status_slot = slot;
    view
}

#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn timed_bucket(
    label: &str,
    used_label: Option<String>,
    limit_label: Option<String>,
    remaining_percent: Option<(CanonicalRemainingPercent, PercentObservationOrigin)>,
    reset_at: Option<i64>,
    now: i64,
    pace_label: Option<&str>,
    status: UsageSnapshotStatus,
) -> QuotaBucketView {
    let mut view = bucket(
        label,
        used_label,
        limit_label,
        remaining_percent,
        reset_at.map(|epoch| reset_label(epoch, now)),
        pace_label,
        status,
    );
    view.resets_at = reset_at;
    view
}
