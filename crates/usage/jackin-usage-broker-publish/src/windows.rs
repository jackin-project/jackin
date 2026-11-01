// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::RawPercentPoints;
use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, StatusSlot, UsageSnapshotStatus,
};
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageFreshnessPhaseV1, UsageFreshnessV1, UsageGenerationView,
    UsageIdentityKindV1, UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1, UsageLimitWindowV1,
    UsageRefreshPhase, UsageWindowCategoryV1,
};

use super::{failure_lifecycle, lifecycle, metric_groups_for_view};

use super::{AccountIdentityMetadata, issue_code, issue_recoverability, quota_state_for_bucket};
use jackin_usage_provider_core::{
    count_summary_label, money_used_percent, provider_quota_state_summary, select_quota_meter,
};

pub fn account_for_view(
    view: &UsageGenerationView,
    rank: usize,
    identity_metadata: Option<&AccountIdentityMetadata>,
) -> Result<UsageAccountV1, String> {
    let snapshot = view.snapshot.clone();
    let header_label = snapshot
        .as_ref()
        .map(|snapshot| snapshot.account.account_label.clone())
        .unwrap_or_default();
    let provider_label = snapshot
        .as_ref()
        .map(|snapshot| snapshot.account.provider_label.clone())
        .unwrap_or_default();
    let display_label = if header_label.trim().is_empty() {
        if provider_label.trim().is_empty() {
            view.capability.surface_id.clone()
        } else {
            provider_label.clone()
        }
    } else {
        header_label.clone()
    };
    let lifecycle = snapshot.as_ref().map_or_else(
        || {
            view.error
                .as_ref()
                .map_or(UsageLifecycleV1::Unavailable, |error| {
                    failure_lifecycle(error.kind)
                })
        },
        |snapshot| lifecycle(snapshot.status, snapshot.confidence),
    );
    let is_stale = snapshot.as_ref().is_some_and(|snapshot| {
        matches!(snapshot.status, UsageSnapshotStatus::Stale)
            || view.phase == UsageRefreshPhase::Failed
    });
    let phase = if view.phase.is_active() {
        UsageFreshnessPhaseV1::Refreshing
    } else if view.snapshot.is_none() {
        UsageFreshnessPhaseV1::Failed
    } else if is_stale {
        UsageFreshnessPhaseV1::Stale
    } else {
        UsageFreshnessPhaseV1::Current
    };
    let (windows, metric_groups) = if let Some(snapshot) = &snapshot {
        (
            windows_for_snapshot(&view.capability.account_id, snapshot)?,
            metric_groups_for_view(
                &view.capability.account_id,
                snapshot,
                snapshot.account.plan_label.as_deref(),
            )?,
        )
    } else {
        (Vec::new(), Vec::new())
    };
    let issues = view
        .error
        .as_ref()
        .map(|error| {
            vec![UsageIssueV1 {
                code: issue_code(error.kind),
                scope: UsageIssueScopeV1::Account,
                recoverability: issue_recoverability(error.kind),
                message: error.message.clone(),
                retry_at_epoch: view.retry_at_epoch,
            }]
        })
        .unwrap_or_default();
    let fallback_identity_kind = UsageIdentityKindV1::UnverifiedHandle;
    Ok(UsageAccountV1 {
        canonical_account_id: view.capability.account_id.clone(),
        identity_kind: identity_metadata
            .map_or(fallback_identity_kind, |metadata| metadata.identity_kind),
        rank: u32::try_from(rank).unwrap_or(u32::MAX),
        display_label,
        plan_label: snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.account.plan_label.clone()),
        status_label: None,
        lifecycle,
        freshness: UsageFreshnessV1 {
            generation: view.generation,
            phase,
            last_good_at_epoch: snapshot.as_ref().map(|snapshot| snapshot.fetched_at_epoch),
            retry_at_epoch: view.retry_at_epoch,
            is_stale,
        },
        provenance_count: identity_metadata.map_or(1, |metadata| metadata.provenance_count),
        windows,
        metric_groups,
        credential_expires_at_epoch: None,
        issues,
    })
}

pub fn windows_for_snapshot(
    account_id: &str,
    snapshot: &FocusedUsageView,
) -> Result<Vec<UsageLimitWindowV1>, String> {
    snapshot
        .buckets
        .iter()
        .enumerate()
        .map(|(rank, bucket)| window_for_bucket(account_id, rank, bucket))
        .collect()
}

pub fn window_for_bucket(
    account_id: &str,
    rank: usize,
    bucket: &QuotaBucketView,
) -> Result<UsageLimitWindowV1, String> {
    let category = match bucket.status_slot {
        Some(StatusSlot::Session) => UsageWindowCategoryV1::Session,
        Some(StatusSlot::Daily | StatusSlot::Weekly) => UsageWindowCategoryV1::LongRange,
        Some(StatusSlot::Spend) | None => UsageWindowCategoryV1::Other,
    };
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
        window_id: format!("{account_id}:{rank}"),
        rank: u32::try_from(rank).unwrap_or(u32::MAX),
        category,
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
        quota_state: quota_state_for_bucket(bucket),
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
        (None, Some(limit)) => limit.clone(),
        (None, None) => money_used_percent.map_or_else(String::new, |used| {
            format!("{}% used", used.display_value())
        }),
    }
}
