// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{UsageScreenState, summary_category_rank};
use std::time::Duration;

use jackin_protocol::control::{
    CountQuota, MeterPercentPoints, PercentObservationOrigin, ProviderQuotaState, RawPercentPoints,
    UsageMeterSide,
};
use jackin_protocol::usage_broker::{
    UsageFreshnessPhaseV1, UsageIdentityKindV1, UsageIssueV1, UsageLifecycleV1,
    UsageMetricGroupKindV1, UsageMetricScopeV1, UsageMetricValueV1, UsageQuotaStateV1,
    UsageWindowCategoryV1,
};

pub const USAGE_HEARTBEAT_INTERVAL: Duration = Duration::from_mins(2);

pub type UsageRefreshOutcome = Result<UsageScreenState, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageRefreshRequest {
    pub generation: u64,
    pub force: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum UsageSort {
    #[default]
    Provider,
    Remaining,
    Name,
}

impl UsageSort {
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            Self::Provider => Self::Remaining,
            Self::Remaining => Self::Name,
            Self::Name => Self::Provider,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::Remaining => "remaining",
            Self::Name => "name",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum UsageFilter {
    #[default]
    All,
    Issues,
    Stale,
}

impl UsageFilter {
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            Self::All => Self::Issues,
            Self::Issues => Self::Stale,
            Self::Stale => Self::All,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Issues => "issues",
            Self::Stale => "stale",
        }
    }

    #[must_use]
    pub fn matches(self, account: &UsageAccount) -> bool {
        match self {
            Self::All => true,
            Self::Issues => account.issue_count() > 0,
            Self::Stale => {
                account.is_stale
                    || matches!(
                        account.freshness_phase,
                        UsageFreshnessPhaseV1::Stale | UsageFreshnessPhaseV1::Failed
                    )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageWindow {
    pub window_id: String,
    pub rank: u32,
    pub category: UsageWindowCategoryV1,
    pub label: String,
    pub value: String,
    pub reset: String,
    pub remaining_percent: Option<RawPercentPoints>,
    pub remaining_percent_origin: Option<PercentObservationOrigin>,
    pub used_percent: Option<RawPercentPoints>,
    pub used_percent_origin: Option<PercentObservationOrigin>,
    pub meter_percent: Option<MeterPercentPoints>,
    pub meter_side: Option<UsageMeterSide>,
    pub count_quota: Option<CountQuota>,
    pub provider_quota_state: Option<ProviderQuotaState>,
    pub money_used_percent: Option<RawPercentPoints>,
    pub reset_at_epoch: Option<i64>,
    pub quota_state: UsageQuotaStateV1,
    pub pace_label: Option<String>,
}

impl UsageWindow {
    #[must_use]
    pub fn meter_percent(&self) -> Option<MeterPercentPoints> {
        self.meter_percent
    }

    pub fn meter_geometry(&self) -> Option<(MeterPercentPoints, UsageMeterSide)> {
        Some((self.meter_percent?, self.meter_side?))
    }

    fn remaining_for_sort(&self) -> Option<f64> {
        jackin_usage_provider_core::quota_source_remaining_rank_points(
            self.count_quota,
            self.used_percent,
            self.used_percent_origin,
            self.remaining_percent,
            self.remaining_percent_origin,
            self.money_used_percent,
            self.provider_quota_state,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageMetricGroup {
    pub group_id: String,
    pub rank: u32,
    pub kind: UsageMetricGroupKindV1,
    pub label: String,
    pub scope: UsageMetricScopeV1,
    pub observed_at_epoch: Option<i64>,
    pub fetched_at_epoch: i64,
    pub last_success_at_epoch: Option<i64>,
    pub phase: UsageFreshnessPhaseV1,
    pub is_stale: bool,
    pub quota_state: UsageQuotaStateV1,
    pub value: UsageMetricValueV1,
    pub reset_at_epoch: Option<i64>,
    pub renews_at_epoch: Option<i64>,
    pub issues: Vec<UsageIssueV1>,
}

impl UsageMetricGroup {
    #[must_use]
    pub fn meter_percent(&self) -> Option<MeterPercentPoints> {
        if let UsageMetricValueV1::Window { meter_percent, .. } = &self.value {
            *meter_percent
        } else {
            None
        }
    }

    pub fn meter_geometry(&self) -> Option<(MeterPercentPoints, UsageMeterSide)> {
        if let UsageMetricValueV1::Window {
            meter_percent,
            meter_side,
            ..
        } = &self.value
        {
            Some(((*meter_percent)?, (*meter_side)?))
        } else {
            None
        }
    }

    fn remaining_for_sort(&self) -> Option<f64> {
        if let UsageMetricValueV1::Window {
            remaining_percent,
            remaining_percent_origin,
            used_percent,
            used_percent_origin,
            count_quota,
            money_used_percent,
            provider_quota_state,
            ..
        } = &self.value
        {
            jackin_usage_provider_core::quota_source_remaining_rank_points(
                *count_quota,
                *used_percent,
                *used_percent_origin,
                *remaining_percent,
                *remaining_percent_origin,
                *money_used_percent,
                *provider_quota_state,
            )
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageAccount {
    pub provider_id: String,
    pub canonical_account_id: String,
    pub unresolved: bool,
    pub provider: String,
    pub account: String,
    pub status: String,
    pub lifecycle: UsageLifecycleV1,
    pub freshness_phase: UsageFreshnessPhaseV1,
    pub last_good_at_epoch: Option<i64>,
    pub retry_at_epoch: Option<i64>,
    pub is_stale: bool,
    pub identity_kind: Option<UsageIdentityKindV1>,
    pub plan_label: Option<String>,
    pub credential_expires_at_epoch: Option<i64>,
    pub issues: Vec<UsageIssueV1>,
    pub provider_issues: Vec<UsageIssueV1>,
    pub windows: Vec<UsageWindow>,
    pub metric_groups: Vec<UsageMetricGroup>,
}

impl UsageAccount {
    #[must_use]
    pub fn stable_id(&self) -> String {
        format!("{}:{}", self.provider_id, self.canonical_account_id)
    }

    #[must_use]
    pub fn issue_count(&self) -> usize {
        self.issues.len()
            + self.provider_issues.len()
            + self
                .metric_groups
                .iter()
                .map(|group| group.issues.len())
                .sum::<usize>()
    }

    #[must_use]
    pub fn min_remaining(&self) -> Option<f64> {
        self.windows
            .iter()
            .filter_map(UsageWindow::remaining_for_sort)
            .chain(
                self.metric_groups
                    .iter()
                    .filter_map(UsageMetricGroup::remaining_for_sort),
            )
            .min_by(f64::total_cmp)
    }

    #[must_use]
    pub fn summary_window(&self) -> Option<&UsageWindow> {
        self.windows
            .iter()
            .filter(|window| {
                window.meter_geometry().is_some()
                    || window.quota_state == UsageQuotaStateV1::NotApplicable
                    || window.provider_quota_state.is_some()
                    || window.used_percent.is_some()
                    || window.remaining_percent.is_some()
                    || window.money_used_percent.is_some()
                    || window.count_quota.is_some_and(|count| {
                        count.used.is_some() || count.limit.is_some() || count.remaining.is_some()
                    })
            })
            .min_by_key(|window| (summary_category_rank(window.category), window.rank))
    }

    #[must_use]
    pub(crate) fn soonest_reset_epoch(&self) -> Option<i64> {
        self.windows
            .iter()
            .filter_map(|window| window.reset_at_epoch)
            .chain(
                self.metric_groups
                    .iter()
                    .filter_map(|group| group.reset_at_epoch),
            )
            .chain(
                self.metric_groups
                    .iter()
                    .filter_map(|group| group.renews_at_epoch),
            )
            .min()
    }
}
