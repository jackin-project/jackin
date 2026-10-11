// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Account window projection builders.

use jackin_core::account_key_hash;
use jackin_protocol::control::{QuotaBucketView, StatusSlot};
use jackin_protocol::usage_broker::{
    UsageCoordinationErrorKind, UsageLifecycleV1, UsageLimitWindowV1, UsagePercent,
    UsageWindowCategoryV1,
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
    let raw_used = money_used_raw_percent(bucket);
    let overage = raw_used.is_some_and(|value| value > 100);
    let (remaining_percent, remaining_raw_percent) = if overage {
        (None, None)
    } else if let Some(value) = bucket.remaining_percent {
        let (raw, clamped) = UsagePercent::split_raw(i32::from(value));
        (Some(clamped), Some(raw))
    } else {
        (None, None)
    };
    let (used_percent, used_raw_percent) = if overage || remaining_percent.is_none() {
        if let Some(raw) = raw_used {
            let (_, clamped) = UsagePercent::split_raw(raw);
            (Some(clamped), Some(raw))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };
    let value_label = if overage {
        raw_used.map_or_else(
            || bucket.used_label.clone().unwrap_or_default(),
            |raw| format!("{raw}% used"),
        )
    } else {
        bucket.remaining_percent.map_or_else(
            || bucket.used_label.clone().unwrap_or_default(),
            |value| format!("{value}% left"),
        )
    };
    Ok(UsageLimitWindowV1 {
        window_id: account_key_hash(canonical_account_id, &format!("canonical-window-v1:{rank}")),
        rank: u32::try_from(rank).map_err(|_| "window rank overflow")?,
        category: window_category(bucket.status_slot),
        label: bucket.label.clone(),
        value_label,
        reset_label: bucket.reset_label.clone().unwrap_or_default(),
        remaining_percent,
        remaining_raw_percent,
        used_percent,
        used_raw_percent,
        reset_at_epoch: bucket.resets_at,
        quota_state: quota_state(bucket),
        pace_label: bucket.pace_label.clone(),
        // No run-out signal exists outside the provider pace composite (which
        // already reaches both surfaces via `pace_label`); the field stays
        // unset rather than deriving a burn-rate estimate no producer stands
        // behind.
        runs_out_label: None,
    })
}

pub const fn window_category(status_slot: Option<StatusSlot>) -> UsageWindowCategoryV1 {
    match status_slot {
        Some(StatusSlot::Daily | StatusSlot::Weekly) => UsageWindowCategoryV1::LongRange,
        Some(StatusSlot::Session) => UsageWindowCategoryV1::Session,
        Some(StatusSlot::Spend) | None => UsageWindowCategoryV1::Other,
    }
}

/// Raw used percentage from a monetary bucket, unclamped so over-100% overage
/// survives. One shared [`Money::raw_percent_of`] rule with the capsule
/// bucket presentation, so both surfaces recover the same overage magnitude.
fn money_used_raw_percent(bucket: &QuotaBucketView) -> Option<i32> {
    bucket
        .used_money
        .as_ref()?
        .raw_percent_of(bucket.limit_money.as_ref()?)
}
