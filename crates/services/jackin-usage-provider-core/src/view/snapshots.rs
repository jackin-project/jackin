// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{
    AccountUsageSnapshotView, CachedUsage, HashMap, QuotaBucketView, UsageSnapshotStatus,
    usage_confidence_storage_label, usage_source_storage_label, usage_status_storage_label,
};

pub fn account_snapshot_views_from_cache(
    snapshots: &HashMap<String, CachedUsage>,
) -> Vec<AccountUsageSnapshotView> {
    let mut accounts = snapshots
        .values()
        .flat_map(|cached| {
            let view = &cached.view;
            view.buckets.iter().map(|bucket| {
                let (used_amount, used_unit, limit_amount, limit_unit) =
                    quota_amounts_for_account_snapshot(bucket);
                let status =
                    if snapshot_status_rank(bucket.status) > snapshot_status_rank(view.status) {
                        bucket.status
                    } else {
                        view.status
                    };
                AccountUsageSnapshotView {
                    provider: view.account.provider_label.clone(),
                    account_label: view.account.account_label.clone(),
                    source: usage_source_storage_label(view.source).to_owned(),
                    confidence: usage_confidence_storage_label(view.confidence).to_owned(),
                    window_kind: bucket.label.clone(),
                    used_amount,
                    used_unit,
                    limit_amount,
                    limit_unit,
                    resets_at: bucket.resets_at,
                    fetched_at: view.fetched_at_epoch,
                    expires_at: None,
                    status: usage_status_storage_label(status).to_owned(),
                    last_error: view.last_error.clone(),
                    quota_bucket: Some(bucket.clone()),
                }
            })
        })
        .collect::<Vec<_>>();
    accounts.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then(left.window_kind.cmp(&right.window_kind))
    });
    accounts
}

/// Preserve the legacy CLI scalar summary; `quota_bucket` is authoritative for
/// new consumers and retains the exact typed observations.
pub fn quota_amounts_for_account_snapshot(
    bucket: &QuotaBucketView,
) -> (Option<i64>, Option<String>, Option<i64>, Option<String>) {
    if bucket.used_money.is_some() || bucket.limit_money.is_some() {
        return (
            bucket.used_money.as_ref().map(|money| money.amount_minor),
            bucket
                .used_money
                .as_ref()
                .map(|money| money.currency.clone()),
            bucket.limit_money.as_ref().map(|money| money.amount_minor),
            bucket
                .limit_money
                .as_ref()
                .map(|money| money.currency.clone()),
        );
    }
    // Percentages and counts belong to the exact typed bucket payload. The
    // legacy signed scalar columns are reserved for the independent Money
    // record path and must not quantize new quota observations.
    (None, None, None, None)
}

pub(crate) const fn snapshot_status_rank(status: UsageSnapshotStatus) -> u8 {
    match status {
        UsageSnapshotStatus::Fresh => 0,
        UsageSnapshotStatus::Stale => 1,
        UsageSnapshotStatus::NeedsLogin => 2,
        UsageSnapshotStatus::NeedsSecret => 3,
        UsageSnapshotStatus::Unsupported => 4,
        UsageSnapshotStatus::Unavailable => 5,
        UsageSnapshotStatus::Error => 6,
    }
}
