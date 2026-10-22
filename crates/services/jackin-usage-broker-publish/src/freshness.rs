// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{QuotaBucketView, UsageSnapshotStatus};
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageFreshnessPhaseV1, UsageFreshnessV1, UsageQuotaStateV1,
};

pub use super::quota::{bucket_has_quantity, money_is_exhausted};

pub fn freshness(status: UsageSnapshotStatus, last_good: i64, generation: u64) -> UsageFreshnessV1 {
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

pub fn provider_freshness(accounts: &[UsageAccountV1], generation: u64) -> UsageFreshnessV1 {
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

pub fn quota_state(bucket: &QuotaBucketView) -> UsageQuotaStateV1 {
    super::quota::quota_state_for_bucket(bucket)
}

pub const fn status_label(status: UsageSnapshotStatus) -> &'static str {
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
