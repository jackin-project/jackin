// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::account_key_hash;
use jackin_protocol::control::{
    Money, ProviderQuotaState, QuotaBucketView, StatusSlot, UsageConfidence, UsageSnapshotStatus,
};
use jackin_protocol::usage_broker::{
    UsageCalendarPeriodV1, UsageFreshnessPhaseV1, UsageLifecycleV1, UsageMetricGroupKindV1,
    UsageMetricGroupV1, UsageMetricPeriodV1, UsageMetricScopeV1, UsageMetricValueV1,
    UsageQuotaStateV1,
};

use super::project_window;

pub fn metric_groups_for_view(
    canonical_account_id: &str,
    view: &jackin_protocol::control::FocusedUsageView,
    plan_label: Option<&str>,
) -> Result<Vec<UsageMetricGroupV1>, String> {
    project_groups(view, plan_label, canonical_account_id)
}

pub fn project_groups(
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

pub fn group_id(canonical_account_id: &str, rank: usize) -> String {
    account_key_hash(canonical_account_id, &format!("canonical-group-v1:{rank}"))
}

pub fn group_rank(rank: usize) -> Result<u32, String> {
    u32::try_from(rank).map_err(|_| "metric group rank overflow".to_owned())
}

pub fn group_epochs(view_fetched_at: i64, usable: bool) -> (Option<i64>, Option<i64>) {
    let observed = Some(view_fetched_at);
    let last_success = usable.then_some(view_fetched_at);
    (observed, last_success)
}

pub fn project_window_group(
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
            remaining_percent_origin: window.remaining_percent_origin,
            used_percent: window.used_percent,
            used_percent_origin: window.used_percent_origin,
            meter_percent: window.meter_percent,
            meter_side: window.meter_side,
            count_quota: window.count_quota,
            provider_quota_state: window.provider_quota_state,
            money_used_percent: window.money_used_percent,
            period: group_period_for_bucket(bucket),
            unit: bucket
                .count_quota
                .and_then(|count| count.unit.machine_unit().map(str::to_owned)),
        },
        reset_at_epoch: bucket.resets_at,
        renews_at_epoch: None,
        issues: Vec::new(),
    })
}

pub fn project_spend_group(
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

pub fn project_plan_group(
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
        quota_state: UsageQuotaStateV1::NotApplicable,
        value: UsageMetricValueV1::Plan {
            plan_label: Some(plan_label.to_owned()),
            tier: None,
        },
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    })
}

pub fn view_is_usable(status: UsageSnapshotStatus) -> bool {
    matches!(
        status,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
    )
}

pub fn group_phase(
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

pub fn group_period(status_slot: Option<StatusSlot>) -> UsageMetricPeriodV1 {
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

fn group_period_for_bucket(bucket: &QuotaBucketView) -> UsageMetricPeriodV1 {
    match bucket.count_quota.map(|count| count.period) {
        Some(jackin_protocol::control::CountQuotaPeriod::UtcDaily) => {
            UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Daily,
            }
        }
        Some(jackin_protocol::control::CountQuotaPeriod::Unknown) => UsageMetricPeriodV1::Unknown,
        None => group_period(bucket.status_slot),
    }
}

pub fn spend_remaining(bucket: &QuotaBucketView) -> Option<Money> {
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

pub fn spend_quota_state(bucket: &QuotaBucketView) -> UsageQuotaStateV1 {
    match bucket.status {
        UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::NeedsSecret => {
            UsageQuotaStateV1::NoPermission
        }
        UsageSnapshotStatus::Unsupported => UsageQuotaStateV1::Unsupported,
        UsageSnapshotStatus::Unavailable => UsageQuotaStateV1::Unavailable,
        UsageSnapshotStatus::Error => UsageQuotaStateV1::Error,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => {
            match bucket.provider_quota_state {
                Some(ProviderQuotaState::Exhausted) => UsageQuotaStateV1::Exhausted,
                Some(ProviderQuotaState::Unlimited) => UsageQuotaStateV1::NotApplicable,
                None => match (bucket.used_money.as_ref(), bucket.limit_money.as_ref()) {
                    (Some(used), Some(limit)) => spend_ratio_state(used, limit),
                    (Some(_), None) => UsageQuotaStateV1::NotApplicable,
                    _ => UsageQuotaStateV1::Unknown,
                },
            }
        }
    }
}

pub fn spend_ratio_state(used: &Money, limit: &Money) -> UsageQuotaStateV1 {
    if used.currency != limit.currency || used.exponent != limit.exponent || limit.amount_minor <= 0
    {
        return UsageQuotaStateV1::Unknown;
    }
    if used.amount_minor >= limit.amount_minor {
        UsageQuotaStateV1::Exhausted
    } else if i128::from(used.amount_minor) * 100 / i128::from(limit.amount_minor) >= 80 {
        UsageQuotaStateV1::Warning
    } else {
        UsageQuotaStateV1::Available
    }
}

pub fn lifecycle(status: UsageSnapshotStatus, confidence: UsageConfidence) -> UsageLifecycleV1 {
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
