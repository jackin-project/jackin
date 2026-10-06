// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Metric groups.

use jackin_core::account_key_hash;
use jackin_protocol::control::{
    Money, QuotaBucketView, StatusSlot, UsageConfidence, UsageSnapshotStatus,
};
use jackin_protocol::usage_broker::{
    UsageCalendarPeriodV1, UsageFreshnessPhaseV1, UsageLifecycleV1, UsageMetricGroupKindV1,
    UsageMetricGroupV1, UsageMetricPeriodV1, UsageMetricScopeV1, UsageMetricValueV1,
    UsageQuotaStateV1,
};

use super::project_window;

/// Build typed metric groups from the existing provider view.
///
/// One window group mirrors each quota bucket; monetary buckets additionally
/// yield a spend-cap group carrying the structured [`Money`] amounts; a
/// provider plan label yields a plan group. Groups reuse the view's fetched
/// timestamp for their own observed/fetched/last-success epochs: current views
/// report transport completion only, so observation time equals fetch time and
/// last success is set exactly when the view holds usable data. Scope labels,
/// balances, token totals, and rate limits stay unset until provider
/// collectors supply them; nothing is inferred.
pub(crate) fn metric_groups_for_view(
    canonical_account_id: &str,
    view: &jackin_protocol::control::FocusedUsageView,
    plan_label: Option<&str>,
) -> Result<Vec<UsageMetricGroupV1>, String> {
    project_groups(view, plan_label, canonical_account_id)
}

pub(crate) fn project_groups(
    view: &jackin_protocol::control::FocusedUsageView,
    plan_label: Option<&str>,
    canonical_account_id: &str,
) -> Result<Vec<UsageMetricGroupV1>, String> {
    let mut groups = Vec::new();
    for bucket in &view.buckets {
        let rank = groups.len();
        groups.push(project_window_group(
            canonical_account_id,
            bucket,
            view.status,
            view.fetched_at_epoch,
            rank,
        )?);
        if bucket.used_money.is_some() || bucket.limit_money.is_some() {
            let rank = groups.len();
            groups.push(project_spend_group(
                canonical_account_id,
                bucket,
                view.status,
                view.fetched_at_epoch,
                rank,
            )?);
        }
    }
    if let Some(plan_label) = plan_label {
        let rank = groups.len();
        groups.push(project_plan_group(
            canonical_account_id,
            view.status,
            view.fetched_at_epoch,
            plan_label,
            rank,
        )?);
    }
    for (group_rank, group) in groups.iter().enumerate() {
        group.validate(group_rank)?;
    }
    Ok(groups)
}

pub(crate) fn group_id(canonical_account_id: &str, rank: usize) -> String {
    account_key_hash(canonical_account_id, &format!("canonical-group-v1:{rank}"))
}

pub(crate) fn group_rank(rank: usize) -> Result<u32, String> {
    u32::try_from(rank).map_err(|_| "metric group rank overflow".to_owned())
}

/// Per-group timestamps from a view that reports transport completion only.
pub(crate) fn group_epochs(view_fetched_at: i64, usable: bool) -> (Option<i64>, Option<i64>) {
    let observed = Some(view_fetched_at);
    let last_success = usable.then_some(view_fetched_at);
    (observed, last_success)
}

pub(crate) fn project_window_group(
    canonical_account_id: &str,
    bucket: &QuotaBucketView,
    view_status: UsageSnapshotStatus,
    view_fetched_at: i64,
    rank: usize,
) -> Result<UsageMetricGroupV1, String> {
    let window = project_window(canonical_account_id, bucket, rank)?;
    let phase = group_phase(bucket.status, view_status);
    let (observed_at_epoch, last_success_at_epoch) =
        group_epochs(view_fetched_at, view_is_usable(bucket.status));
    Ok(UsageMetricGroupV1 {
        group_id: group_id(canonical_account_id, rank),
        rank: group_rank(rank)?,
        kind: UsageMetricGroupKindV1::Window,
        label: bucket.label.clone(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch,
        fetched_at_epoch: view_fetched_at,
        last_success_at_epoch,
        phase,
        is_stale: phase == UsageFreshnessPhaseV1::Stale,
        quota_state: window.quota_state,
        value: UsageMetricValueV1::Window {
            remaining_percent: window.remaining_percent,
            remaining_raw_percent: window.remaining_raw_percent,
            used_percent: window.used_percent,
            used_raw_percent: window.used_raw_percent,
            period: group_period(bucket.status_slot),
            unit: None,
        },
        reset_at_epoch: bucket.resets_at,
        renews_at_epoch: None,
        issues: Vec::new(),
    })
}

pub(crate) fn project_spend_group(
    canonical_account_id: &str,
    bucket: &QuotaBucketView,
    view_status: UsageSnapshotStatus,
    view_fetched_at: i64,
    rank: usize,
) -> Result<UsageMetricGroupV1, String> {
    let phase = group_phase(bucket.status, view_status);
    let (observed_at_epoch, last_success_at_epoch) =
        group_epochs(view_fetched_at, view_is_usable(bucket.status));
    let quota_state = spend_quota_state(bucket);
    Ok(UsageMetricGroupV1 {
        group_id: group_id(canonical_account_id, rank),
        rank: group_rank(rank)?,
        kind: UsageMetricGroupKindV1::SpendCap,
        label: format!("{} spend", bucket.label),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch,
        fetched_at_epoch: view_fetched_at,
        last_success_at_epoch,
        phase,
        is_stale: phase == UsageFreshnessPhaseV1::Stale,
        quota_state,
        value: UsageMetricValueV1::SpendCap {
            cap: bucket.limit_money.clone(),
            spent: bucket.used_money.clone(),
            remaining: spend_remaining(bucket),
        },
        reset_at_epoch: bucket.resets_at,
        renews_at_epoch: None,
        issues: Vec::new(),
    })
}

pub(crate) fn project_plan_group(
    canonical_account_id: &str,
    view_status: UsageSnapshotStatus,
    view_fetched_at: i64,
    plan_label: &str,
    rank: usize,
) -> Result<UsageMetricGroupV1, String> {
    let phase = group_phase(view_status, view_status);
    let (observed_at_epoch, last_success_at_epoch) =
        group_epochs(view_fetched_at, view_is_usable(view_status));
    Ok(UsageMetricGroupV1 {
        group_id: group_id(canonical_account_id, rank),
        rank: group_rank(rank)?,
        kind: UsageMetricGroupKindV1::Plan,
        label: "Plan".to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch,
        fetched_at_epoch: view_fetched_at,
        last_success_at_epoch,
        phase,
        is_stale: phase == UsageFreshnessPhaseV1::Stale,
        // Plan metadata carries no quota notion.
        quota_state: UsageQuotaStateV1::NotApplicable,
        value: UsageMetricValueV1::Plan {
            plan_label: Some(plan_label.to_owned()),
            tier: None,
        },
        reset_at_epoch: None,
        // No renewal signal exists in current provider views.
        renews_at_epoch: None,
        issues: Vec::new(),
    })
}

pub(crate) fn view_is_usable(status: UsageSnapshotStatus) -> bool {
    matches!(
        status,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
    )
}

pub(crate) fn group_phase(
    bucket_status: UsageSnapshotStatus,
    view_status: UsageSnapshotStatus,
) -> UsageFreshnessPhaseV1 {
    if view_status == UsageSnapshotStatus::Stale {
        return UsageFreshnessPhaseV1::Stale;
    }
    match bucket_status {
        UsageSnapshotStatus::Fresh => UsageFreshnessPhaseV1::Current,
        UsageSnapshotStatus::Stale => UsageFreshnessPhaseV1::Stale,
        _ => UsageFreshnessPhaseV1::Failed,
    }
}

pub(crate) fn group_period(status_slot: Option<StatusSlot>) -> UsageMetricPeriodV1 {
    match status_slot {
        Some(StatusSlot::Session) => UsageMetricPeriodV1::ProviderDefined,
        Some(StatusSlot::Daily) => UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Daily,
        },
        Some(StatusSlot::Weekly) => UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Weekly,
        },
        Some(StatusSlot::Spend) | None => UsageMetricPeriodV1::Unknown,
    }
}

/// Remaining spend from a monetary bucket. Checked subtraction on compatible
/// denominations only; over-spend clamps at zero here because a Money amount
/// cannot express negative remaining, while the over-100% raw percent on the
/// sibling window payload preserves the overage magnitude.
pub(crate) fn spend_remaining(bucket: &QuotaBucketView) -> Option<Money> {
    let used = bucket.used_money.as_ref()?;
    let limit = bucket.limit_money.as_ref()?;
    if used.currency != limit.currency || used.exponent != limit.exponent {
        return None;
    }
    let remaining = limit.amount_minor.saturating_sub(used.amount_minor).max(0);
    Some(Money::new(
        remaining,
        limit.currency.clone(),
        limit.exponent,
    ))
}

/// Quota state for a spend-cap group from its money ratio. A missing or
/// unusable cap is [`UsageQuotaStateV1::Unknown`], never fabricated credit;
/// an uncapped tracker is [`UsageQuotaStateV1::NotApplicable`].
pub(crate) fn spend_quota_state(bucket: &QuotaBucketView) -> UsageQuotaStateV1 {
    match bucket.status {
        UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::NeedsSecret => {
            UsageQuotaStateV1::NoPermission
        }
        UsageSnapshotStatus::Unsupported => UsageQuotaStateV1::Unsupported,
        UsageSnapshotStatus::Unavailable => UsageQuotaStateV1::Unavailable,
        UsageSnapshotStatus::Error => UsageQuotaStateV1::Error,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => {
            match (bucket.used_money.as_ref(), bucket.limit_money.as_ref()) {
                (Some(used), Some(limit)) => spend_ratio_state(used, limit),
                // Spend without a cap is uncapped tracking; a cap without
                // spend leaves the ratio unknown.
                (Some(_), None) => UsageQuotaStateV1::NotApplicable,
                _ => UsageQuotaStateV1::Unknown,
            }
        }
    }
}

/// Quota state from a spend/cap money ratio with checked math.
pub(crate) fn spend_ratio_state(used: &Money, limit: &Money) -> UsageQuotaStateV1 {
    if used.currency != limit.currency || used.exponent != limit.exponent || limit.amount_minor <= 0
    {
        return UsageQuotaStateV1::Unknown;
    }
    if used.amount_minor >= limit.amount_minor {
        UsageQuotaStateV1::Exhausted
    } else if used.amount_minor.saturating_mul(100) / limit.amount_minor >= 80 {
        UsageQuotaStateV1::Warning
    } else {
        UsageQuotaStateV1::Available
    }
}

pub(in crate::host) fn lifecycle(
    status: UsageSnapshotStatus,
    confidence: UsageConfidence,
) -> UsageLifecycleV1 {
    if confidence == UsageConfidence::PresenceOnly {
        return UsageLifecycleV1::AgentUninitialized;
    }
    match status {
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => UsageLifecycleV1::Available,
        UsageSnapshotStatus::NeedsLogin => UsageLifecycleV1::NeedsLogin,
        UsageSnapshotStatus::NeedsSecret => UsageLifecycleV1::NeedsSecret,
        UsageSnapshotStatus::Unsupported => UsageLifecycleV1::Unsupported,
        UsageSnapshotStatus::Unavailable => UsageLifecycleV1::Unavailable,
        UsageSnapshotStatus::Error => UsageLifecycleV1::Error,
    }
}
