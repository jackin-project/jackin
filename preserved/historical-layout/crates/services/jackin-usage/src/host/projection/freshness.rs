// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Freshness and quota states.

use jackin_protocol::control::{QuotaBucketView, UsageSeverity, UsageSnapshotStatus};
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageFreshnessPhaseV1, UsageFreshnessV1, UsageQuotaStateV1,
};

pub(crate) fn freshness(
    status: UsageSnapshotStatus,
    last_good: i64,
    generation: u64,
) -> UsageFreshnessV1 {
    let phase = match status {
        UsageSnapshotStatus::Fresh => UsageFreshnessPhaseV1::Current,
        UsageSnapshotStatus::Stale => UsageFreshnessPhaseV1::Stale,
        _ => UsageFreshnessPhaseV1::Failed,
    };
    UsageFreshnessV1 {
        generation,
        phase,
        last_good_at_epoch: matches!(
            status,
            UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
        )
        .then_some(last_good),
        retry_at_epoch: None,
        is_stale: status == UsageSnapshotStatus::Stale,
    }
}

pub(crate) fn provider_freshness(accounts: &[UsageAccountV1], generation: u64) -> UsageFreshnessV1 {
    let is_stale = accounts.iter().any(|account| account.freshness.is_stale);
    let phase = if accounts
        .iter()
        .any(|account| account.freshness.phase == UsageFreshnessPhaseV1::Refreshing)
    {
        UsageFreshnessPhaseV1::Refreshing
    } else if is_stale {
        UsageFreshnessPhaseV1::Stale
    } else if accounts.is_empty()
        || accounts
            .iter()
            .all(|account| account.freshness.phase == UsageFreshnessPhaseV1::Failed)
    {
        UsageFreshnessPhaseV1::Failed
    } else {
        UsageFreshnessPhaseV1::Current
    };
    UsageFreshnessV1 {
        generation,
        phase,
        last_good_at_epoch: accounts
            .iter()
            .filter_map(|account| account.freshness.last_good_at_epoch)
            .max(),
        retry_at_epoch: accounts
            .iter()
            .filter_map(|account| account.freshness.retry_at_epoch)
            .min(),
        is_stale,
    }
}

/// Quota state for one bucket with no collapsing: missing permission stays
/// [`UsageQuotaStateV1::NoPermission`] (never [`UsageQuotaStateV1::Unsupported`]),
/// and a fresh bucket with no usable quantity stays
/// [`UsageQuotaStateV1::Unknown`] (never a fabricated `0%` bar or an
/// [`UsageQuotaStateV1::Available`] claim).
pub(crate) fn quota_state(bucket: &QuotaBucketView) -> UsageQuotaStateV1 {
    match bucket.status {
        UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::NeedsSecret => {
            UsageQuotaStateV1::NoPermission
        }
        UsageSnapshotStatus::Unsupported => UsageQuotaStateV1::Unsupported,
        UsageSnapshotStatus::Unavailable => UsageQuotaStateV1::Unavailable,
        UsageSnapshotStatus::Error => UsageQuotaStateV1::Error,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => {
            if bucket.remaining_percent == Some(0) || money_is_exhausted(bucket) {
                UsageQuotaStateV1::Exhausted
            } else {
                match bucket.severity {
                    UsageSeverity::Danger => UsageQuotaStateV1::Exhausted,
                    UsageSeverity::Warn => UsageQuotaStateV1::Warning,
                    UsageSeverity::Normal => {
                        if bucket_has_quantity(bucket) {
                            UsageQuotaStateV1::Available
                        } else {
                            UsageQuotaStateV1::Unknown
                        }
                    }
                }
            }
        }
    }
}

/// Whether a monetary bucket reports spend at or over its cap on a compatible
/// denomination. Incompatible or unusable money never reads as exhausted.
pub(crate) fn money_is_exhausted(bucket: &QuotaBucketView) -> bool {
    match (bucket.used_money.as_ref(), bucket.limit_money.as_ref()) {
        (Some(used), Some(limit)) => {
            used.currency == limit.currency
                && used.exponent == limit.exponent
                && limit.amount_minor > 0
                && used.amount_minor >= limit.amount_minor
        }
        _ => false,
    }
}

/// Whether a bucket carries any usable quantity: a percent, a money amount,
/// or a provider quantity label. Buckets with none of these are unknown, not
/// available.
pub(crate) fn bucket_has_quantity(bucket: &QuotaBucketView) -> bool {
    bucket.remaining_percent.is_some()
        || bucket.used_money.is_some()
        || bucket.limit_money.is_some()
        || bucket.used_label.is_some()
        || bucket.limit_label.is_some()
}

pub(crate) const fn status_label(status: UsageSnapshotStatus) -> &'static str {
    match status {
        UsageSnapshotStatus::Fresh => "Available",
        UsageSnapshotStatus::Stale => "Stale",
        UsageSnapshotStatus::NeedsLogin => "Needs login",
        UsageSnapshotStatus::NeedsSecret => "Needs secret",
        UsageSnapshotStatus::Unsupported => "Unsupported",
        UsageSnapshotStatus::Unavailable => "Unavailable",
        UsageSnapshotStatus::Error => "Error",
    }
}
