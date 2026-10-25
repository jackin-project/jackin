// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::account_key_hash;
use jackin_protocol::control::{QuotaBucketView, RawPercentPoints, StatusSlot};
use jackin_protocol::usage_broker::{
    UsageCoordinationErrorKind, UsageLifecycleV1, UsageLimitWindowV1, UsageWindowCategoryV1,
};
use jackin_usage_provider_core::{
    count_summary_label, money_used_percent, provider_quota_state_summary, select_quota_meter,
};

use super::quota_state;

pub fn failure_lifecycle(kind: UsageCoordinationErrorKind) -> UsageLifecycleV1 {
    match kind {
        UsageCoordinationErrorKind::NeedsSecret => UsageLifecycleV1::NeedsSecret,
        UsageCoordinationErrorKind::Unauthorized => UsageLifecycleV1::NeedsLogin,
        UsageCoordinationErrorKind::ProtocolMismatch => UsageLifecycleV1::Unsupported,
        UsageCoordinationErrorKind::Unavailable
        | UsageCoordinationErrorKind::ProviderUnavailable => UsageLifecycleV1::Unavailable,
        _ => UsageLifecycleV1::Error,
    }
}

pub fn project_window(
    canonical_account_id: &str,
    bucket: &QuotaBucketView,
    rank: usize,
) -> Result<UsageLimitWindowV1, String> {
    bucket.validate()?;
    let remaining_percent = bucket
        .remaining_percent
        .map(|percent| RawPercentPoints::new(percent.get()))
        .transpose()?;
    let used_percent = bucket
        .used_percent
        .map(|percent| RawPercentPoints::new(percent.get()))
        .transpose()?;
    let money_used_percent = money_used_percent(bucket);
    let value_label = window_value_label(bucket, money_used_percent);
    let (meter_percent, meter_side) = select_quota_meter(bucket);
    Ok(UsageLimitWindowV1 {
        window_id: account_key_hash(canonical_account_id, &format!("canonical-window-v1:{rank}")),
        rank: u32::try_from(rank).map_err(|_| "window rank overflow")?,
        category: window_category(bucket.status_slot),
        label: bucket.label.clone(),
        value_label,
        reset_label: bucket.reset_label.clone().unwrap_or_default(),
        remaining_percent,
        remaining_percent_origin: bucket.remaining_percent_origin,
        used_percent,
        used_percent_origin: bucket.used_percent_origin,
        meter_percent,
        meter_side,
        count_quota: bucket.count_quota,
        provider_quota_state: bucket.provider_quota_state,
        money_used_percent,
        reset_at_epoch: bucket.resets_at,
        quota_state: quota_state(bucket),
        pace_label: bucket.pace_label.clone(),
        runs_out_label: None,
    })
}

fn window_value_label(
    bucket: &QuotaBucketView,
    money_used_percent: Option<RawPercentPoints>,
) -> String {
    if let Some(summary) = provider_quota_state_summary(bucket) {
        return summary;
    }
    if let Some(money_used) = money_used_percent.filter(|used| used.get() > 100.0) {
        return format!("{}% used", money_used.display_value());
    }
    if let Some(used) = bucket.used_percent {
        return used.label();
    }
    if let Some(remaining) = bucket.remaining_percent {
        return remaining.label();
    }
    if let Some(count) = count_summary_label(bucket) {
        return count;
    }
    match (&bucket.used_label, &bucket.limit_label) {
        (Some(used), Some(limit)) => format!("{used} of {limit}"),
        (Some(used), None) => used.clone(),

        (None, Some(_)) => String::new(),
        (None, None) => money_used_percent.map_or_else(String::new, |used| {
            format!("{}% used", used.display_value())
        }),
    }
}

pub const fn window_category(status_slot: Option<StatusSlot>) -> UsageWindowCategoryV1 {
    match status_slot {
        Some(StatusSlot::Daily | StatusSlot::Weekly) => UsageWindowCategoryV1::LongRange,
        Some(StatusSlot::Session) => UsageWindowCategoryV1::Session,
        Some(StatusSlot::Spend) | None => UsageWindowCategoryV1::Other,
    }
}
