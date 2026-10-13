// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage screen model types.

use super::{UsageScreenState, summary_category_rank};
use std::time::Duration;

use jackin_protocol::usage_broker::{
    UsageFreshnessPhaseV1, UsageIdentityKindV1, UsageIssueV1, UsageLifecycleV1,
    UsageMetricGroupKindV1, UsageMetricScopeV1, UsageMetricValueV1, UsagePercent,
    UsageQuotaStateV1, UsageWindowCategoryV1,
};

/// Heartbeat cadence while the Usage route stays open: a due refresh is
/// requested at most every two minutes (the pre-existing direct-interaction
/// cadence). Broker single-flight generations dedup overlapping requests;
/// completions always reset the timer, so sleep never causes a catch-up
/// burst — at most one refresh is ever in flight.
pub const USAGE_HEARTBEAT_INTERVAL: Duration = Duration::from_mins(2);

/// Completed background refresh payload: the complete publication. Produced off the UI thread by
/// `load_console_usage_state` in the console adapter.
pub type UsageRefreshOutcome = Result<UsageScreenState, String>;

/// One due Usage refresh, following the instance-refresh effect+subscription
/// pattern: the worker tags its outcome with `generation` and
/// [`UsageScreenState::poll_refresh`] drops completions from stale
/// generations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageRefreshRequest {
    pub generation: u64,
    /// True only for an explicit operator refresh (`r`): bypasses the broker
    /// success cadence. Periodic and open-path refreshes pass false so broker
    /// cadence and retry deadlines win. Broker-owned rate-limit/`Retry-After`
    /// deadlines are honored either way, and active generations are joined
    /// rather than duplicated.
    pub force: bool,
}

/// Console list sort order, cycled with `s`. `Provider` is the default: the
/// projection order is already provider-grouped (see `from_projection`), so
/// it renders as-is and refreshes never reorder under the operator.
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

/// Console list filter predicate, cycled with `f`. Pure membership over one
/// account; the account list, the overview panel, and the capacity-finder all
/// observe the same visible set.
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
    pub remaining_percent: Option<u8>,
    pub remaining_raw_percent: Option<i32>,
    pub used_percent: Option<u8>,
    pub used_raw_percent: Option<i32>,
    pub reset_at_epoch: Option<i64>,
    pub quota_state: UsageQuotaStateV1,
    pub pace_label: Option<String>,
}

impl UsageWindow {
    /// Meter geometry input. Providers report exactly one representation;
    /// `used` is mirrored to `remaining` without inventing precision.
    /// `None` means unknown — callers must render no bar at all.
    #[must_use]
    pub fn meter_percent(&self) -> Option<u8> {
        self.remaining_percent.or_else(|| {
            self.used_percent
                .map(|used| 100_u8.saturating_sub(used.min(100)))
        })
    }
}

/// One independently fetched typed metric group, mirroring the canonical
/// [`jackin_protocol::usage_broker::UsageMetricGroupV1`] field for field.
/// Reset (`reset_at_epoch`), renewal (`renews_at_epoch`), and balance-expiry
/// timestamps stay separate facts and are never merged at render time.
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
    /// Meter geometry for window-kind groups only. Every other kind carries
    /// no percentage and must render no bar at all — never a fabricated one.
    #[must_use]
    pub fn meter_percent(&self) -> Option<u8> {
        if let UsageMetricValueV1::Window {
            remaining_percent,
            used_percent,
            ..
        } = &self.value
        {
            remaining_percent
                .map(UsagePercent::get)
                .or_else(|| used_percent.map(|used| 100_u8.saturating_sub(used.get().min(100))))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageAccount {
    pub provider_id: String,
    /// Canonical account id; the capability id while `unresolved`.
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
    /// Non-secret evidence kind backing the canonical id. `None` while
    /// `unresolved`: no identity evidence exists yet.
    pub identity_kind: Option<UsageIdentityKindV1>,
    pub plan_label: Option<String>,
    /// Credential/auth-session expiry. Never a quota reset and never a
    /// subscription renewal — those live on windows and plan groups.
    pub credential_expires_at_epoch: Option<i64>,
    /// Sanitized account/window-scoped issues.
    pub issues: Vec<UsageIssueV1>,
    /// Sanitized provider-scoped issues for this account's provider group.
    pub provider_issues: Vec<UsageIssueV1>,
    pub windows: Vec<UsageWindow>,
    pub metric_groups: Vec<UsageMetricGroup>,
}

impl UsageAccount {
    /// Stable selection identity across rename/reorder. Display labels
    /// never participate: renames keep the id, and unresolved rows anchor
    /// on their capability id.
    #[must_use]
    pub fn stable_id(&self) -> String {
        format!("{}:{}", self.provider_id, self.canonical_account_id)
    }

    /// Total sanitized issue count across account, provider, and group scopes.
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

    /// Minimum known remaining percent across principal windows and metered
    /// (window-kind) metric groups. `None` when nothing reports a percent —
    /// unknown quota never fabricates a value, so capacity sort and the
    /// capacity-finder always rank it last, never as zero.
    #[must_use]
    pub fn min_remaining(&self) -> Option<u8> {
        self.windows
            .iter()
            .filter_map(UsageWindow::meter_percent)
            .chain(
                self.metric_groups
                    .iter()
                    .filter_map(UsageMetricGroup::meter_percent),
            )
            .min()
    }

    /// First available Rust-ranked limit (D30: long-range, model-specific,
    /// session, then other; ties break to provider order). Metered and explicitly
    /// unlimited windows qualify. The list summary and its meter bar both read this one
    /// window, matching the capsule tab status selection. Explicit unlimited
    /// windows remain meaningful without a percentage.
    #[must_use]
    pub fn summary_window(&self) -> Option<&UsageWindow> {
        self.windows
            .iter()
            .filter(|window| {
                window.meter_percent().is_some()
                    || window.quota_state == UsageQuotaStateV1::NotApplicable
            })
            .min_by_key(|window| (summary_category_rank(window.category), window.rank))
    }

    /// Soonest known reset/renewal epoch across windows and metric groups.
    /// Tie-break input for the capacity-finder only; `None` sorts last.
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
