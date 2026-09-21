// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Simple Console Usage route.
//!
//! Rust supplies already ordered account/window/group values. This module owns
//! only the Console split, focus, and Capsule-shaped meter adaptation.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent};
use jackin_protocol::usage_broker::{
    UsageCalendarPeriodV1, UsageFreshnessPhaseV1, UsageIdentityKindV1, UsageIssueV1,
    UsageLifecycleV1, UsageMetricGroupKindV1, UsageMetricPeriodV1, UsageMetricScopeV1,
    UsageMetricValueV1, UsagePercent, UsageQuotaStateV1, UsageWindowCategoryV1,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::tui::runtime::{BlockingSubscription, SubscriptionPoll};
use crate::tui::state::ManagerState;

/// Heartbeat cadence while the Usage route stays open: a due refresh is
/// requested at most every two minutes (the pre-existing direct-interaction
/// cadence). Broker single-flight generations dedup overlapping requests;
/// completions always reset the timer, so sleep never causes a catch-up
/// burst — at most one refresh is ever in flight.
pub const USAGE_HEARTBEAT_INTERVAL: Duration = Duration::from_mins(2);

/// Completed background refresh payload: canonical rows plus the
/// projection-level notice. Produced off the UI thread by
/// `load_console_usage_state` in the console adapter.
pub type UsageRefreshOutcome = Result<(Vec<UsageAccount>, Option<String>), String>;

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
    /// session, then other; ties break to provider order). Only metered
    /// windows qualify. The list summary and its meter bar both read this one
    /// window, matching the capsule tab status selection.
    #[must_use]
    pub fn summary_window(&self) -> Option<&UsageWindow> {
        self.windows
            .iter()
            .filter(|window| window.meter_percent().is_some())
            .min_by_key(|window| (summary_category_rank(window.category), window.rank))
    }

    /// Soonest known reset/renewal epoch across windows and metric groups.
    /// Tie-break input for the capacity-finder only; `None` sorts last.
    #[must_use]
    fn soonest_reset_epoch(&self) -> Option<i64> {
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

#[derive(Debug, Default)]
pub struct UsageScreenState {
    pub accounts: Vec<UsageAccount>,
    pub selected: usize,
    pub selected_id: Option<String>,
    pub detail: bool,
    pub sort: UsageSort,
    pub filter: UsageFilter,
    pub scroll: u16,
    pub notice: Option<String>,
    pub generated_at_epoch: Option<i64>,
    /// Sanitized projection-scoped issues from the latest publication.
    pub projection_issues: Vec<UsageIssueV1>,
    pub refresh_due: bool,
    pub force_refresh_pending: bool,
    pub refresh_generation: u64,
    pub last_refresh_at: Option<Instant>,
    pub refresh_rx: Option<BlockingSubscription<(u64, UsageRefreshOutcome)>>,
}

// Manual impls: the in-flight refresh handle carries no value identity.
// Cloning drops it (the worker result is then discarded); equality ignores
// it so tests can compare screen snapshots while a refresh runs. The
// generation counter is value state and survives both.
impl Clone for UsageScreenState {
    fn clone(&self) -> Self {
        Self {
            accounts: self.accounts.clone(),
            selected: self.selected,
            selected_id: self.selected_id.clone(),
            detail: self.detail,
            sort: self.sort,
            filter: self.filter,
            scroll: self.scroll,
            notice: self.notice.clone(),
            generated_at_epoch: self.generated_at_epoch,
            projection_issues: self.projection_issues.clone(),
            refresh_due: self.refresh_due,
            force_refresh_pending: self.force_refresh_pending,
            refresh_generation: self.refresh_generation,
            last_refresh_at: self.last_refresh_at,
            refresh_rx: None,
        }
    }
}

impl PartialEq for UsageScreenState {
    fn eq(&self, other: &Self) -> bool {
        self.accounts == other.accounts
            && self.selected == other.selected
            && self.selected_id == other.selected_id
            && self.detail == other.detail
            && self.sort == other.sort
            && self.filter == other.filter
            && self.scroll == other.scroll
            && self.notice == other.notice
            && self.generated_at_epoch == other.generated_at_epoch
            && self.projection_issues == other.projection_issues
            && self.refresh_due == other.refresh_due
            && self.force_refresh_pending == other.force_refresh_pending
            && self.refresh_generation == other.refresh_generation
            && self.last_refresh_at == other.last_refresh_at
    }
}

impl Eq for UsageScreenState {}

impl UsageScreenState {
    /// Open the route over a cached snapshot and mark a refresh due so the
    /// first broker read lands right after open. Selection starts at
    /// Overview; refreshes re-anchor by stable id from there.
    #[must_use]
    pub fn open_with_snapshot(accounts: Vec<UsageAccount>, notice: Option<String>) -> Self {
        Self {
            accounts,
            notice,
            refresh_due: true,
            ..Self::default()
        }
    }

    /// Project the Rust-owned canonical publication into Console rows.
    ///
    /// The Console owns only layout. Provider/account identity, lifecycle,
    /// ordering, and quota labels remain in the protocol projection.
    pub fn from_projection(projection: &jackin_protocol::usage_broker::UsageProjectionV1) -> Self {
        let mut accounts = Vec::new();
        for provider in &projection.providers {
            for account in &provider.accounts {
                let mut status = account
                    .status_label
                    .clone()
                    .unwrap_or_else(|| lifecycle_label(account.lifecycle).to_owned());
                if account.freshness.is_stale && account.lifecycle == UsageLifecycleV1::Available {
                    status = "stale".to_owned();
                }
                let windows = account
                    .windows
                    .iter()
                    .map(|window| UsageWindow {
                        window_id: window.window_id.clone(),
                        rank: window.rank,
                        category: window.category,
                        label: window.label.clone(),
                        value: window.value_label.clone(),
                        reset: window.reset_label.clone(),
                        remaining_percent: window.remaining_percent.map(UsagePercent::get),
                        remaining_raw_percent: window.remaining_raw_percent,
                        used_percent: window.used_percent.map(UsagePercent::get),
                        used_raw_percent: window.used_raw_percent,
                        reset_at_epoch: window.reset_at_epoch,
                        quota_state: window.quota_state,
                        pace_label: window.pace_label.clone(),
                    })
                    .collect();
                let metric_groups = account
                    .metric_groups
                    .iter()
                    .map(|group| UsageMetricGroup {
                        group_id: group.group_id.clone(),
                        rank: group.rank,
                        kind: group.kind,
                        label: group.label.clone(),
                        scope: group.scope.clone(),
                        observed_at_epoch: group.observed_at_epoch,
                        fetched_at_epoch: group.fetched_at_epoch,
                        last_success_at_epoch: group.last_success_at_epoch,
                        phase: group.phase,
                        is_stale: group.is_stale,
                        quota_state: group.quota_state,
                        value: group.value.clone(),
                        reset_at_epoch: group.reset_at_epoch,
                        renews_at_epoch: group.renews_at_epoch,
                        issues: group.issues.clone(),
                    })
                    .collect();
                accounts.push(UsageAccount {
                    provider_id: provider.provider_id.clone(),
                    canonical_account_id: account.canonical_account_id.clone(),
                    unresolved: false,
                    provider: provider.display_name.clone(),
                    account: account.display_label.clone(),
                    status,
                    lifecycle: account.lifecycle,
                    freshness_phase: account.freshness.phase,
                    last_good_at_epoch: account.freshness.last_good_at_epoch,
                    retry_at_epoch: account.freshness.retry_at_epoch,
                    is_stale: account.freshness.is_stale,
                    identity_kind: Some(account.identity_kind),
                    plan_label: account.plan_label.clone(),
                    credential_expires_at_epoch: account.credential_expires_at_epoch,
                    issues: account.issues.clone(),
                    provider_issues: provider.issues.clone(),
                    windows,
                    metric_groups,
                });
            }
        }

        for unresolved in &projection.unresolved {
            let provider_name = projection
                .providers
                .iter()
                .find(|p| p.provider_id == unresolved.provider_id)
                .map_or_else(
                    || well_known_provider_name(&unresolved.provider_id),
                    |p| p.display_name.clone(),
                );
            let account_label = format!("Unresolved ({})", unresolved.capability_id);
            let mut status = lifecycle_label(unresolved.state).to_owned();
            if let Some(issue) = unresolved
                .issues
                .first()
                .filter(|issue| !issue.message.trim().is_empty())
            {
                status.push_str(" · ");
                status.push_str(issue.message.trim());
            }
            accounts.push(UsageAccount {
                provider_id: unresolved.provider_id.clone(),
                canonical_account_id: unresolved.capability_id.clone(),
                unresolved: true,
                provider: provider_name,
                account: account_label,
                status,
                lifecycle: unresolved.state,
                freshness_phase: UsageFreshnessPhaseV1::Failed,
                last_good_at_epoch: None,
                retry_at_epoch: None,
                is_stale: false,
                identity_kind: None,
                plan_label: None,
                credential_expires_at_epoch: None,
                issues: unresolved.issues.clone(),
                provider_issues: Vec::new(),
                windows: Vec::new(),
                metric_groups: Vec::new(),
            });
        }

        let mut provider_order = Vec::new();
        for account in &accounts {
            if !provider_order.contains(&account.provider) {
                provider_order.push(account.provider.clone());
            }
        }
        accounts.sort_by_key(|account| {
            provider_order
                .iter()
                .position(|p| p == &account.provider)
                .unwrap_or(usize::MAX)
        });

        let notice = if projection.unresolved.is_empty() {
            None
        } else {
            Some(format!(
                "{} configured capability(s) unresolved",
                projection.unresolved.len()
            ))
        };
        Self {
            accounts,
            notice,
            generated_at_epoch: Some(projection.generated_at_epoch),
            projection_issues: projection.issues.clone(),
            ..Self::default()
        }
    }

    /// Apply a completed background refresh, re-anchoring selection by
    /// stable id so renames/reorders keep the operator's row. A removed
    /// selection falls back to Overview with an inline notice.
    pub fn apply_refresh(
        &mut self,
        accounts: Vec<UsageAccount>,
        notice: Option<String>,
        now: Instant,
    ) {
        self.accounts = accounts;
        self.notice = notice;
        self.last_refresh_at = Some(now);
        self.refresh_due = false;
        // A refresh replaces the list: old offsets are meaningless and a
        // stale deep offset would blank the panes until the operator
        // scrolled back (same rule as `reanchor_after_view_change`).
        self.scroll = 0;
        match &self.selected_id {
            None => self.selected = 0,
            Some(id) => {
                if let Some(pos) = self
                    .visible_order()
                    .iter()
                    .position(|&index| self.accounts[index].stable_id() == *id)
                {
                    self.selected = pos.saturating_add(1);
                } else if self.accounts.iter().any(|a| &a.stable_id() == id) {
                    // Still configured but hidden by the current filter: park
                    // on Overview and keep the id so clearing the filter (or
                    // the next refresh) restores the row without a notice.
                    self.selected = 0;
                } else {
                    self.selected = 0;
                    self.selected_id = None;
                    self.notice = Some(match self.notice.take() {
                        Some(notice) => format!(
                            "{notice} · previously selected account unavailable; showing Overview"
                        ),
                        None => {
                            "Previously selected account unavailable; showing Overview".to_owned()
                        }
                    });
                }
            }
        }
        self.selected = self.selected.min(self.visible_order().len());
    }

    /// Record a failed background refresh. The timer still advances so a
    /// broken broker retries at heartbeat cadence (or on manual `r`),
    /// never once per keypress.
    pub fn apply_refresh_error(&mut self, notice: String, now: Instant) {
        self.notice = Some(notice);
        self.last_refresh_at = Some(now);
        self.refresh_due = false;
    }

    #[must_use]
    pub fn refresh_in_flight(&self) -> bool {
        self.refresh_rx.is_some()
    }

    /// True while an empty route is still waiting on broker work: a refresh
    /// is either in flight or due (the open path marks one due before the
    /// worker starts). Empty branches render the loading line in this case
    /// instead of claiming no providers are configured.
    #[must_use]
    pub fn loading(&self) -> bool {
        self.refresh_in_flight() || self.refresh_due
    }

    /// Claim the next due refresh, if any, following the instance-refresh
    /// throttle shape: at most one generation is ever in flight, and a
    /// requester arriving while one runs joins that shared work instead of
    /// queueing a duplicate. Claiming consumes `refresh_due` and the pending
    /// force flag; the worker must tag its outcome with the generation via
    /// [`Self::begin_refresh`].
    pub fn next_refresh_plan_if_due(&mut self, now: Instant) -> Option<UsageRefreshRequest> {
        if self.refresh_in_flight() {
            self.refresh_due = false;
            return None;
        }
        if !self.refresh_due && !self.heartbeat_due(now) {
            return None;
        }
        self.refresh_generation = self.refresh_generation.wrapping_add(1);
        self.refresh_due = false;
        let force = std::mem::take(&mut self.force_refresh_pending);
        Some(UsageRefreshRequest {
            generation: self.refresh_generation,
            force,
        })
    }

    pub fn begin_refresh(&mut self, rx: BlockingSubscription<(u64, UsageRefreshOutcome)>) {
        self.refresh_rx = Some(rx);
    }

    /// Poll the in-flight refresh once. `None` means still running — or that
    /// the completed generation was stale and its outcome was dropped.
    pub fn poll_refresh(&mut self) -> Option<UsageRefreshOutcome> {
        let rx = self.refresh_rx.as_mut()?;
        match rx.poll_next() {
            SubscriptionPoll::Ready((generation, outcome)) => {
                self.refresh_rx = None;
                if generation == self.refresh_generation {
                    Some(outcome)
                } else {
                    None
                }
            }
            SubscriptionPoll::Closed => {
                self.refresh_rx = None;
                Some(Err("usage refresh worker disconnected".to_owned()))
            }
            SubscriptionPoll::Pending => None,
        }
    }

    /// True once the last completed refresh is older than the heartbeat
    /// interval. Never true before the first completion: the open path
    /// drives that via `refresh_due`.
    #[must_use]
    pub fn heartbeat_due(&self, now: Instant) -> bool {
        self.last_refresh_at
            .is_some_and(|at| now.duration_since(at) >= USAGE_HEARTBEAT_INTERVAL)
    }

    /// Canonical-account indices in display order after the current filter
    /// and sort. Selection positions address this list: position 0 is
    /// Overview, position `p > 0` is `visible[p - 1]`. The default
    /// provider/all view is the identity order baked in by `from_projection`.
    #[must_use]
    pub fn visible_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.accounts.len())
            .filter(|&index| self.filter.matches(&self.accounts[index]))
            .collect();
        match self.sort {
            UsageSort::Provider => {}
            UsageSort::Remaining => {
                order.sort_by_key(|&index| self.accounts[index].min_remaining().unwrap_or(u8::MAX));
            }
            UsageSort::Name => order.sort_by_key(|&index| {
                let account = &self.accounts[index];
                (
                    account.account.to_lowercase(),
                    account.provider.to_lowercase(),
                )
            }),
        }
        order
    }

    /// Re-anchor `selected` by stable id after a sort/filter change, so the
    /// operator's row follows the account instead of the position. A row
    /// hidden by the filter parks on Overview with its id kept, so clearing
    /// the filter restores it. Always resets scroll: old offsets are
    /// meaningless once the list reorders.
    fn reanchor_after_view_change(&mut self) {
        let order = self.visible_order();
        match &self.selected_id {
            Some(id) => {
                self.selected = order
                    .iter()
                    .position(|&index| self.accounts[index].stable_id() == *id)
                    .map_or(0, |pos| pos.saturating_add(1));
            }
            None => self.selected = 0,
        }
        self.scroll = 0;
    }

    /// `selected` position of the most-constrained visible account: lowest
    /// known remaining percent, ties to the soonest reset. Accounts that
    /// report no percent never win — unknown quota is not zero quota.
    #[must_use]
    pub fn most_constrained_selected(&self) -> Option<usize> {
        let order = self.visible_order();
        order
            .iter()
            .enumerate()
            .filter(|&(_, &index)| self.accounts[index].min_remaining().is_some())
            .min_by_key(|&(_, &index)| {
                let account = &self.accounts[index];
                (
                    account.min_remaining().unwrap_or(u8::MAX),
                    account.soonest_reset_epoch().unwrap_or(i64::MAX),
                )
            })
            .map(|(pos, _)| pos.saturating_add(1))
    }

    /// Jump selection to the most-constrained visible account. Returns false
    /// — posting an inline notice instead of moving — when there is nothing
    /// to compare: no accounts, an empty filter result, or no known percents.
    pub fn jump_to_most_constrained(&mut self) -> bool {
        if let Some(selected) = self.most_constrained_selected() {
            self.selected = selected;
            self.selected_id = self.selected_account().map(UsageAccount::stable_id);
            self.scroll = 0;
            true
        } else {
            self.notice = Some(if self.accounts.is_empty() {
                "No usage accounts configured; nothing to compare".to_owned()
            } else if self.visible_order().is_empty() {
                format!(
                    "No accounts match filter '{}'; press f to clear",
                    self.filter.label()
                )
            } else {
                "No visible account reports remaining quota".to_owned()
            });
            false
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        let order = self.visible_order();
        if order.is_empty() {
            self.selected = 0;
            return;
        }
        let len = order.len().saturating_add(1);
        let current = self.selected.min(len - 1);
        self.selected = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current
                .saturating_add(delta.cast_unsigned())
                .min(len.saturating_sub(1))
        };
        self.selected_id = if self.selected == 0 {
            None
        } else {
            order
                .get(self.selected.saturating_sub(1))
                .and_then(|&index| self.accounts.get(index))
                .map(UsageAccount::stable_id)
        };
    }

    pub fn selected_account(&self) -> Option<&UsageAccount> {
        if self.selected == 0 {
            return None;
        }
        let order = self.visible_order();
        let index = *order.get(self.selected.saturating_sub(1))?;
        self.accounts.get(index)
    }

    fn overview_selected(&self) -> bool {
        self.selected == 0
    }
}

/// Lifecycle word for one account. The shared vocabulary (`needs login`,
/// `needs secret`, `unsupported`, `unavailable`, `error`) matches Capsule
/// `usage_tab_status_label`; `available`/`not started` have no Capsule
/// snapshot-status counterpart (Capsule says `fresh` for the freshness axis,
/// a different concept) and stay console-owned. See the alignment table test.
fn lifecycle_label(lifecycle: UsageLifecycleV1) -> &'static str {
    match lifecycle {
        UsageLifecycleV1::Available => "available",
        UsageLifecycleV1::AgentUninitialized => "not started",
        UsageLifecycleV1::NeedsLogin => "needs login",
        UsageLifecycleV1::NeedsSecret => "needs secret",
        UsageLifecycleV1::Unsupported => "unsupported",
        UsageLifecycleV1::Unavailable => "unavailable",
        UsageLifecycleV1::Error => "error",
    }
}

/// Quota-state word for one window/group. The Capsule tab vocabulary has no
/// quota axis (`usage_tab_status_label` reports snapshot status plus the
/// `{n}% left` headline, which the console mirrors in its list summary), so
/// these words stay console-owned and are pinned by the alignment table test.
fn quota_state_label(state: UsageQuotaStateV1) -> &'static str {
    match state {
        UsageQuotaStateV1::Available => "available",
        UsageQuotaStateV1::NotStarted => "not started",
        UsageQuotaStateV1::Warning => "warning",
        UsageQuotaStateV1::Exhausted => "exhausted",
        UsageQuotaStateV1::Unsupported => "unsupported",
        UsageQuotaStateV1::Unavailable => "unavailable",
        UsageQuotaStateV1::NoPermission => "no permission",
        UsageQuotaStateV1::Unknown => "unknown",
        UsageQuotaStateV1::NotApplicable => "n/a",
        UsageQuotaStateV1::Error => "error",
    }
}

/// Rank of one window category in the settled Overview-summary order (D30:
/// long-range weekly/daily/monthly, model-specific, session, then other).
const fn summary_category_rank(category: UsageWindowCategoryV1) -> u8 {
    match category {
        UsageWindowCategoryV1::LongRange => 0,
        UsageWindowCategoryV1::Model => 1,
        UsageWindowCategoryV1::Session => 2,
        UsageWindowCategoryV1::Other => 3,
    }
}

fn metric_group_kind_label(kind: UsageMetricGroupKindV1) -> &'static str {
    match kind {
        UsageMetricGroupKindV1::Window => "window",
        UsageMetricGroupKindV1::Balance => "balance",
        UsageMetricGroupKindV1::SpendCap => "spend cap",
        UsageMetricGroupKindV1::TokenTotals => "token totals",
        UsageMetricGroupKindV1::RateLimit => "rate limit",
        UsageMetricGroupKindV1::Plan => "plan",
    }
}

fn identity_kind_label(kind: UsageIdentityKindV1) -> &'static str {
    match kind {
        UsageIdentityKindV1::ProviderAccountId => "provider account id",
        UsageIdentityKindV1::ProviderStableHandle => "provider handle",
    }
}

/// Past-age bucket shared by account and group freshness labels.
fn past_age_label(age_secs: i64) -> String {
    if age_secs < 60 {
        "just now".to_owned()
    } else if age_secs < 3_600 {
        format!("{}m ago", age_secs / 60)
    } else if age_secs < 86_400 {
        format!("{}h ago", age_secs / 3_600)
    } else {
        format!("{}d ago", age_secs / 86_400)
    }
}

/// `updated …` age fragment for freshness labels. The sub-minute bucket reads
/// `updated now`, matching Capsule `relative_updated_label` ("Updated now")
/// modulo the console's lowercase row style.
fn updated_age_label(age_secs: i64) -> String {
    if age_secs < 60 {
        "updated now".to_owned()
    } else {
        format!("updated {}", past_age_label(age_secs))
    }
}

/// Relative time for a reset/renewal/expiry/retry epoch against an explicit
/// `now`. Pure so tests stay deterministic; render passes wall-clock time.
/// Past mirrors the freshness buckets; future uses `in …` buckets.
fn relative_time_label(now_epoch: i64, epoch: i64) -> String {
    if epoch >= now_epoch {
        let ahead = epoch.saturating_sub(now_epoch);
        if ahead < 60 {
            "in under a minute".to_owned()
        } else if ahead < 3_600 {
            format!("in {}m", ahead / 60)
        } else if ahead < 86_400 {
            format!("in {}h", ahead / 3_600)
        } else {
            format!("in {}d", ahead / 86_400)
        }
    } else {
        past_age_label(now_epoch.saturating_sub(epoch))
    }
}

/// Credential/auth-session expiry as one relative fact. Derived only from the
/// canonical expiry epoch: already-passed reads `expired …`, future reads
/// `expires …`. Never confused with a quota reset or subscription renewal.
fn credential_expiry_label(now_epoch: i64, expires_at_epoch: i64) -> String {
    if expires_at_epoch < now_epoch {
        format!(
            "expired {}",
            past_age_label(now_epoch.saturating_sub(expires_at_epoch))
        )
    } else {
        format!(
            "expires {}",
            relative_time_label(now_epoch, expires_at_epoch)
        )
    }
}

/// Operator-facing freshness age for one metric group. Staleness is per
/// group: a fresh sibling never makes retained old data fresh.
#[must_use]
pub fn group_freshness_label(now_epoch: i64, group: &UsageMetricGroup) -> String {
    if group.phase == UsageFreshnessPhaseV1::Refreshing {
        return "refreshing…".to_owned();
    }
    let Some(last_success) = group.last_success_at_epoch else {
        return "never updated".to_owned();
    };
    let updated = updated_age_label(now_epoch.saturating_sub(last_success).max(0));
    if group.is_stale
        || matches!(
            group.phase,
            UsageFreshnessPhaseV1::Stale | UsageFreshnessPhaseV1::Failed
        )
    {
        format!("stale · {updated}")
    } else {
        updated
    }
}

fn duration_label(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3_600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3_600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

fn metric_period_label(period: &UsageMetricPeriodV1) -> Option<String> {
    match period {
        UsageMetricPeriodV1::Rolling { window_secs } => {
            Some(format!("rolling {}", duration_label(*window_secs)))
        }
        UsageMetricPeriodV1::Calendar { granularity } => Some(
            match granularity {
                UsageCalendarPeriodV1::Daily => "daily",
                UsageCalendarPeriodV1::Weekly => "weekly",
                UsageCalendarPeriodV1::Monthly => "monthly",
            }
            .to_owned(),
        ),
        UsageMetricPeriodV1::ProviderDefined => Some("provider-defined period".to_owned()),
        UsageMetricPeriodV1::Unknown => None,
    }
}

/// One percent side (`remaining` or `used`) as display text. The raw provider
/// value rides along only when it differs from clamped geometry, so overage
/// stays honest without duplicating equal values.
fn percent_side_summary(clamped: Option<u8>, raw: Option<i32>, word: &str) -> Option<String> {
    match (clamped, raw) {
        (Some(percent), Some(raw)) if i32::from(percent) != raw => {
            Some(format!("{percent}% {word} (raw {raw}%)"))
        }
        (Some(percent), _) => Some(format!("{percent}% {word}")),
        (None, Some(raw)) => Some(format!("raw {raw}% {word}")),
        (None, None) => None,
    }
}

fn window_percent_summary(
    remaining: Option<u8>,
    remaining_raw: Option<i32>,
    used: Option<u8>,
    used_raw: Option<i32>,
) -> Option<String> {
    // "left" matches the principal-window value labels (projection-owned)
    // and the Capsule bucket presentation; "remaining" would be a third word
    // for the same meaning (S4/S5 parity).
    percent_side_summary(remaining, remaining_raw, "left")
        .or_else(|| percent_side_summary(used, used_raw, "used"))
}

/// Raw-percent note for a principal window, shown only when a raw value is
/// present and differs from the clamped geometry the bar uses.
fn raw_percent_note(window: &UsageWindow) -> Option<String> {
    for (clamped, raw, word) in [
        (
            window.remaining_percent,
            window.remaining_raw_percent,
            "remaining",
        ),
        (window.used_percent, window.used_raw_percent, "used"),
    ] {
        if let Some(raw) = raw
            && clamped.is_none_or(|percent| i32::from(percent) != raw)
        {
            return Some(format!("raw {word} {raw}%"));
        }
    }
    None
}

/// One-line typed value summary for a metric group. `None` means the provider
/// supplied no displayable value — callers render no value line at all rather
/// than a fabricated zero.
fn metric_group_value_summary(group: &UsageMetricGroup) -> Option<String> {
    match &group.value {
        UsageMetricValueV1::Window {
            remaining_percent,
            remaining_raw_percent,
            used_percent,
            used_raw_percent,
            period,
            unit,
        } => {
            let mut parts = Vec::new();
            if let Some(percent) = window_percent_summary(
                remaining_percent.map(UsagePercent::get),
                *remaining_raw_percent,
                used_percent.map(UsagePercent::get),
                *used_raw_percent,
            ) {
                parts.push(percent);
            }
            if let Some(unit) = unit.as_deref().filter(|unit| !unit.trim().is_empty()) {
                parts.push((*unit).to_owned());
            }
            if let Some(period) = metric_period_label(period) {
                parts.push(period);
            }
            (!parts.is_empty()).then(|| parts.join(" · "))
        }
        UsageMetricValueV1::Balance { amount, .. } => Some(amount.to_string()),
        UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            let mut parts = Vec::new();
            match cap {
                Some(cap) => parts.push(format!("cap {cap}")),
                None => parts.push("uncapped".to_owned()),
            }
            if let Some(spent) = spent {
                parts.push(format!("spent {spent}"));
            }
            if let Some(remaining) = remaining {
                parts.push(format!("remaining {remaining}"));
            }
            Some(parts.join(" · "))
        }
        UsageMetricValueV1::TokenTotals {
            input,
            output,
            cached,
            reasoning,
            interval_label,
        } => {
            let mut parts = Vec::new();
            for (count, word) in [
                (*input, "input"),
                (*output, "output"),
                (*cached, "cached"),
                (*reasoning, "reasoning"),
            ] {
                if let Some(count) = count {
                    parts.push(format!("{word} {count}"));
                }
            }
            if let Some(label) = interval_label
                .as_deref()
                .filter(|label| !label.trim().is_empty())
            {
                parts.push((*label).to_owned());
            }
            (!parts.is_empty()).then(|| parts.join(" · "))
        }
        UsageMetricValueV1::RateLimit {
            limit,
            remaining,
            window_label,
        } => {
            let mut parts = Vec::new();
            if let Some(limit) = limit {
                parts.push(format!("limit {limit}"));
            }
            if let Some(remaining) = remaining {
                parts.push(format!("remaining {remaining}"));
            }
            if let Some(label) = window_label
                .as_deref()
                .filter(|label| !label.trim().is_empty())
            {
                parts.push((*label).to_owned());
            }
            (!parts.is_empty()).then(|| parts.join(" · "))
        }
        UsageMetricValueV1::Plan { plan_label, tier } => {
            let mut parts = Vec::new();
            if let Some(label) = plan_label
                .as_deref()
                .filter(|label| !label.trim().is_empty())
            {
                parts.push((*label).to_owned());
            }
            if let Some(tier) = tier.as_ref().filter(|tier| !tier.trim().is_empty()) {
                parts.push(format!("tier {tier}"));
            }
            (!parts.is_empty()).then(|| parts.join(" · "))
        }
    }
}

/// Non-secret scope labels locating a group inside its account. `None` means
/// the provider did not scope the group on any axis.
fn metric_scope_summary(scope: &UsageMetricScopeV1) -> Option<String> {
    let mut parts = Vec::new();
    for (label, word) in [
        (&scope.service, "service"),
        (&scope.model, "model"),
        (&scope.pool, "pool"),
        (&scope.key_id, "key"),
    ] {
        if let Some(label) = label.as_ref().filter(|label| !label.trim().is_empty()) {
            parts.push(format!("{word} {label}"));
        }
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// One sanitized issue as display text: the Rust-owned operator message with
/// its stable code, plus a broker-owned retry time when one is present.
fn issue_text(issue: &UsageIssueV1, now_epoch: i64) -> String {
    let mut text = match (
        issue.message.trim().is_empty(),
        issue.code.trim().is_empty(),
    ) {
        (false, false) => format!("{} ({})", issue.message.trim(), issue.code.trim()),
        (false, true) => issue.message.trim().to_owned(),
        (true, false) => issue.code.trim().to_owned(),
        (true, true) => "issue".to_owned(),
    };
    if let Some(retry_at) = issue.retry_at_epoch {
        text.push_str(&format!(
            " · retry {}",
            relative_time_label(now_epoch, retry_at)
        ));
    }
    text
}

fn non_empty_label(label: Option<&String>) -> Option<&str> {
    label
        .map(String::as_str)
        .filter(|label| !label.trim().is_empty())
}

/// Operator-facing freshness age for one account. Pure over an explicit
/// `now` so tests stay deterministic; render passes wall-clock time.
#[must_use]
pub fn freshness_age_label(now_epoch: i64, account: &UsageAccount) -> String {
    if account.freshness_phase == UsageFreshnessPhaseV1::Refreshing {
        return "refreshing…".to_owned();
    }
    let Some(last_good) = account.last_good_at_epoch else {
        return "never updated".to_owned();
    };
    let age_secs = now_epoch.saturating_sub(last_good).max(0);
    let updated = updated_age_label(age_secs);
    if account.is_stale
        || matches!(
            account.freshness_phase,
            UsageFreshnessPhaseV1::Stale | UsageFreshnessPhaseV1::Failed
        )
    {
        format!("stale · {updated}")
    } else {
        updated
    }
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Fallback display name for an unresolved provider id. Mirrors Capsule
/// `provider_display_label`: `Anthropic` (not `Anthropic / Claude`) and `xAI`
/// (not `Grok`); every other well-known id already matches the Capsule
/// spelling, and unknown ids pass through untouched.
fn well_known_provider_name(provider_id: &str) -> String {
    match provider_id.to_ascii_lowercase().as_str() {
        "anthropic" | "claude" => "Anthropic".to_owned(),
        "openai" | "codex" => "OpenAI".to_owned(),
        "opencode" => "OpenCode".to_owned(),
        "kimi" | "moonshot" => "Kimi".to_owned(),
        "grok" | "xai" => "xAI".to_owned(),
        "amp" => "Amp".to_owned(),
        "zai" => "Z.AI".to_owned(),
        "minimax" => "MiniMax".to_owned(),
        other => other.to_owned(),
    }
}

pub fn handle_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let Some(screen) = state.usage.screen.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => state.usage.visible = false,
        KeyCode::Up | KeyCode::Char('k') => screen.move_selection(-1),
        KeyCode::Down | KeyCode::Char('j') => screen.move_selection(1),
        KeyCode::Enter => screen.detail = !screen.detail,
        KeyCode::Char('s') => {
            screen.sort = screen.sort.cycle();
            screen.reanchor_after_view_change();
        }
        KeyCode::Char('f') => {
            screen.filter = screen.filter.cycle();
            screen.reanchor_after_view_change();
        }
        KeyCode::Char('c') => {
            screen.jump_to_most_constrained();
        }
        KeyCode::Char('r' | 'R') => {
            screen.refresh_due = true;
            screen.force_refresh_pending = true;
        }
        KeyCode::PageUp => screen.scroll = screen.scroll.saturating_sub(5),
        KeyCode::PageDown => {
            screen.scroll = screen.scroll.saturating_add(5);
        }
        _ => {}
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    let body = crate::tui::view::workspace_frame_areas(area).body;
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(body);
    render_account_list(frame, columns[0], state);
    render_detail(frame, columns[1], state);
}

fn render_account_list(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    let Some(screen) = state.usage.screen.as_ref() else {
        return;
    };
    let focused = true;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if screen.accounts.is_empty() {
        let text = if screen.loading() {
            "Refreshing usage…"
        } else {
            "No providers configured.\n\nPress R to refresh."
        };
        frame.render_widget(
            Paragraph::new(text)
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    let mut lines = Vec::new();
    let meter_width = inner.width.saturating_sub(8) as usize;
    let now = now_epoch();
    lines.push(Line::from(Span::styled(
        format!(
            "  s:sort({}) f:filter({}) c:constrained",
            screen.sort.label(),
            screen.filter.label()
        ),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    let order = screen.visible_order();
    if order.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  No accounts match filter '{}'.", screen.filter.label()),
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "  Press f to cycle the filter.",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(
            Paragraph::new(lines)
                .scroll((screen.scroll, 0))
                .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    lines.push(Line::from(Span::styled(
        format!(
            "{}Overview",
            if screen.overview_selected() {
                "▸  "
            } else {
                "   "
            }
        ),
        row_style(screen.overview_selected()),
    )));
    lines.push(Line::from(""));
    for (pos, &index) in order.iter().enumerate() {
        let account = &screen.accounts[index];
        if pos == 0 || screen.accounts[order[pos - 1]].provider != account.provider {
            lines.push(Line::from(Span::styled(
                format!("  {}", account.provider),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        let selected = pos.saturating_add(1) == screen.selected;
        let cursor = if selected { "▸ " } else { "  " };
        let summary = account
            .summary_window()
            .and_then(UsageWindow::meter_percent)
            .map_or_else(
                || account.status.clone(),
                |percent| format!("{percent}% left"),
            );
        lines.push(Line::from(Span::styled(
            format!("  {cursor}{}", account.account),
            row_style(selected),
        )));
        let mut sub = format!(
            "      {} · {} · {}",
            account.status,
            summary,
            freshness_age_label(now, account)
        );
        if let Some(plan) = non_empty_label(account.plan_label.as_ref()) {
            sub.push_str(&format!(" · {plan}"));
        }
        let issue_count = account.issue_count();
        if issue_count > 0 {
            sub.push_str(&format!(
                " · {} issue{}",
                issue_count,
                if issue_count == 1 { "" } else { "s" }
            ));
        }
        lines.push(Line::from(Span::styled(
            sub,
            Style::default().fg(Color::DarkGray),
        )));
        if let Some(window) = account.summary_window()
            && let Some(bar) = meter_line(meter_width, window.meter_percent())
        {
            lines.push(Line::from(Span::styled(
                bar,
                meter_style(window.quota_state),
            )));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((screen.scroll, 0))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    let Some(screen) = state.usage.screen.as_ref() else {
        return;
    };
    let now = now_epoch();
    let Some(account) = screen.selected_account() else {
        if screen.accounts.is_empty() {
            let text = if screen.loading() {
                "Refreshing usage…"
            } else {
                "No providers configured.\n\nPress R to refresh."
            };
            frame.render_widget(
                Paragraph::new(text)
                    .block(panel("Overview"))
                    .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }
        let order = screen.visible_order();
        if order.is_empty() {
            let mut lines = vec![
                Line::from(format!(
                    "No accounts match filter '{}'.",
                    screen.filter.label()
                )),
                Line::from(""),
                Line::from("Press f to cycle the filter."),
            ];
            if let Some(notice) = &screen.notice {
                lines.push(Line::from(Span::styled(
                    notice.clone(),
                    Style::default().fg(Color::Yellow),
                )));
            }
            frame.render_widget(
                Paragraph::new(lines)
                    .block(panel("Overview"))
                    .scroll((screen.scroll, 0))
                    .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }
        let mut lines = vec![Line::from("Status    available"), Line::from("")];
        if screen.refresh_in_flight() {
            lines.push(refreshing_line());
            lines.push(Line::from(""));
        }
        let width = area.width.saturating_sub(8).max(8) as usize;
        for &index in &order {
            append_overview_account(&mut lines, &screen.accounts[index], width, now);
        }
        for issue in &screen.projection_issues {
            lines.push(Line::from(Span::styled(
                format!("  {}", issue_text(issue, now)),
                Style::default().fg(Color::Yellow),
            )));
        }
        if !screen.projection_issues.is_empty() {
            lines.push(Line::from(""));
        }
        if let Some(notice) = &screen.notice {
            lines.push(Line::from(Span::styled(
                notice.clone(),
                Style::default().fg(Color::Yellow),
            )));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(panel("Overview"))
                .scroll((screen.scroll, 0))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    };
    let title = if screen.detail { "Account" } else { "Overview" };
    let mut lines = vec![
        Line::from(format!("Provider  {}", account.provider)),
        Line::from(format!("Account   {}", account.account)),
        Line::from(format!("Status    {}", account.status)),
    ];
    if let Some(plan) = non_empty_label(account.plan_label.as_ref()) {
        lines.push(Line::from(format!("Plan      {plan}")));
    }
    if let Some(identity) = account.identity_kind.map(identity_kind_label) {
        lines.push(Line::from(format!("Identity  {identity}")));
    }
    if let Some(expires_at) = account.credential_expires_at_epoch {
        lines.push(Line::from(format!(
            "Credential {}",
            credential_expiry_label(now, expires_at)
        )));
    }
    let mut freshness = freshness_age_label(now, account);
    if let Some(retry_at) = account.retry_at_epoch {
        freshness.push_str(&format!(" · retry {}", relative_time_label(now, retry_at)));
    }
    lines.push(Line::from(format!("Freshness {freshness}")));
    lines.push(Line::from(""));
    lines.push(Line::from("Limits"));
    if screen.refresh_in_flight() {
        lines.push(refreshing_line());
        lines.push(Line::from(""));
    }
    let width = area.width.saturating_sub(8).max(8) as usize;
    if screen.detail {
        append_account_full_body(&mut lines, account, width, now);
    } else {
        append_account_summary_body(&mut lines, account, width, now);
    }
    if let Some(notice) = &screen.notice {
        lines.push(Line::from(Span::styled(
            notice.clone(),
            Style::default().fg(Color::Yellow),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel(title))
            .scroll((screen.scroll, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// Full account body (`detail`): every window with quota/raw/pace extras,
/// every metric group as a full block, and every issue line.
fn append_account_full_body(
    lines: &mut Vec<Line<'static>>,
    account: &UsageAccount,
    width: usize,
    now_epoch: i64,
) {
    for window in &account.windows {
        lines.push(Line::from(Span::styled(
            format!("  {}", window.label),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        if let Some(bar) = meter_line(width, window.meter_percent()) {
            lines.push(Line::from(Span::styled(
                bar,
                meter_style(window.quota_state),
            )));
        }
        let detail = if window.reset.is_empty() {
            format!("  {}", window.value)
        } else {
            format!("  {} · {}", window.value, window.reset)
        };
        lines.push(Line::from(detail));
        append_window_extra(lines, window);
        lines.push(Line::from(""));
    }
    if !account.metric_groups.is_empty() {
        lines.push(Line::from("Metric groups"));
        for group in &account.metric_groups {
            append_metric_group(lines, group, width, now_epoch);
        }
    }
    if !account.issues.is_empty() || !account.provider_issues.is_empty() {
        lines.push(Line::from("Issues"));
        append_account_issue_lines(lines, account, now_epoch);
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        "Enter for summary",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
}

/// Summary account body: one row per window, compact group rows, and an
/// issue count. Full extras stay behind `Enter`.
fn append_account_summary_body(
    lines: &mut Vec<Line<'static>>,
    account: &UsageAccount,
    width: usize,
    now_epoch: i64,
) {
    for window in &account.windows {
        append_summary_window(lines, window, width);
    }
    for group in &account.metric_groups {
        append_overview_group(lines, group, now_epoch);
    }
    let issue_count = account.issue_count();
    if issue_count > 0 {
        lines.push(Line::from(format!(
            "  {issue_count} issue{} · Enter for detail",
            if issue_count == 1 { "" } else { "s" }
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter for full detail",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
}

fn refreshing_line() -> Line<'static> {
    Line::from(Span::styled(
        "Refreshing usage…",
        Style::default().fg(Color::DarkGray),
    ))
}

/// Quota state, raw-percent, and pace detail shared by the detail view and
/// the overview panel. The `Available` state is silent: it is the default and
/// the meter already shows it.
fn append_window_extra(lines: &mut Vec<Line<'static>>, window: &UsageWindow) {
    if window.quota_state != UsageQuotaStateV1::Available {
        lines.push(Line::from(format!(
            "  quota: {}",
            quota_state_label(window.quota_state)
        )));
    }
    if let Some(note) = raw_percent_note(window) {
        lines.push(Line::from(format!("  {note}")));
    }
    if let Some(pace) = non_empty_label(window.pace_label.as_ref()) {
        lines.push(Line::from(format!("  pace: {pace}")));
    }
}

/// Account- and provider-scoped issue lines shared by the detail view and
/// the overview panel.
fn append_account_issue_lines(
    lines: &mut Vec<Line<'static>>,
    account: &UsageAccount,
    now_epoch: i64,
) {
    for issue in &account.issues {
        lines.push(Line::from(Span::styled(
            format!("  {}", issue_text(issue, now_epoch)),
            Style::default().fg(Color::Yellow),
        )));
    }
    for issue in &account.provider_issues {
        lines.push(Line::from(Span::styled(
            format!("  provider: {}", issue_text(issue, now_epoch)),
            Style::default().fg(Color::Yellow),
        )));
    }
}

/// One window as an account-summary row: label, meter, and value/reset.
/// Quota/raw/pace extras stay in the full view (`detail`).
fn append_summary_window(lines: &mut Vec<Line<'static>>, window: &UsageWindow, width: usize) {
    if !window.label.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", window.label),
            Style::default().fg(Color::DarkGray),
        )));
    }
    if let Some(bar) = meter_line(width, window.meter_percent()) {
        lines.push(Line::from(Span::styled(
            bar,
            meter_style(window.quota_state),
        )));
    }
    let detail = if window.reset.is_empty() {
        format!("  {}", window.value)
    } else {
        format!("  {} · {}", window.value, window.reset)
    };
    lines.push(Line::from(detail));
}

fn append_overview_window(lines: &mut Vec<Line<'static>>, window: &UsageWindow, width: usize) {
    if !window.label.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", window.label),
            Style::default().fg(Color::DarkGray),
        )));
    }
    if let Some(bar) = meter_line(width, window.meter_percent()) {
        lines.push(Line::from(Span::styled(
            bar,
            meter_style(window.quota_state),
        )));
    }
    let detail = if window.reset.is_empty() {
        format!("  {}", window.value)
    } else {
        format!("  {} · {}", window.value, window.reset)
    };
    lines.push(Line::from(detail));
    append_window_extra(lines, window);
}

/// One metric group as a compact overview row: label plus typed value, with
/// reset/renewal facts and group issues on following lines.
fn append_overview_group(lines: &mut Vec<Line<'static>>, group: &UsageMetricGroup, now_epoch: i64) {
    match metric_group_value_summary(group) {
        Some(summary) => lines.push(Line::from(format!("  {}: {summary}", group.label))),
        None => lines.push(Line::from(format!(
            "  {} ({} · {})",
            group.label,
            metric_group_kind_label(group.kind),
            quota_state_label(group.quota_state),
        ))),
    }
    append_group_schedule_lines(lines, group, now_epoch);
    for issue in &group.issues {
        lines.push(Line::from(Span::styled(
            format!("  [{}] {}", group.label, issue_text(issue, now_epoch)),
            Style::default().fg(Color::Yellow),
        )));
    }
}

/// Reset, renewal, and balance-expiry facts for one group. Each timestamp
/// renders under its own word — reset is never shown as renewal or expiry —
/// and absent timestamps render nothing at all.
fn append_group_schedule_lines(
    lines: &mut Vec<Line<'static>>,
    group: &UsageMetricGroup,
    now_epoch: i64,
) {
    if let Some(reset_at) = group.reset_at_epoch {
        lines.push(Line::from(format!(
            "  resets {}",
            relative_time_label(now_epoch, reset_at)
        )));
    }
    if let Some(renews_at) = group.renews_at_epoch {
        lines.push(Line::from(format!(
            "  renews {}",
            relative_time_label(now_epoch, renews_at)
        )));
    }
    if let UsageMetricValueV1::Balance {
        expires_at_epoch: Some(expires_at),
        ..
    } = &group.value
    {
        lines.push(Line::from(format!(
            "  expires {}",
            relative_time_label(now_epoch, *expires_at)
        )));
    }
}

/// One metric group as a full detail block: kind, quota state, and per-group
/// freshness in the header, then scope, typed value, schedule, provenance,
/// and group-scoped issues.
fn append_metric_group(
    lines: &mut Vec<Line<'static>>,
    group: &UsageMetricGroup,
    width: usize,
    now_epoch: i64,
) {
    lines.push(Line::from(Span::styled(
        format!(
            "  {} ({} · {} · {})",
            group.label,
            metric_group_kind_label(group.kind),
            quota_state_label(group.quota_state),
            group_freshness_label(now_epoch, group),
        ),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    if let Some(scope) = metric_scope_summary(&group.scope) {
        lines.push(Line::from(Span::styled(
            format!("  scope: {scope}"),
            Style::default().fg(Color::DarkGray),
        )));
    }
    if let Some(bar) = meter_line(width, group.meter_percent()) {
        lines.push(Line::from(Span::styled(
            bar,
            meter_style(group.quota_state),
        )));
    }
    if let Some(summary) = metric_group_value_summary(group) {
        lines.push(Line::from(format!("  {summary}")));
    }
    append_group_schedule_lines(lines, group, now_epoch);
    let mut fetched = format!(
        "  fetched {}",
        relative_time_label(now_epoch, group.fetched_at_epoch)
    );
    if let Some(observed_at) = group.observed_at_epoch {
        fetched.push_str(&format!(
            " · observed {}",
            relative_time_label(now_epoch, observed_at)
        ));
    }
    lines.push(Line::from(Span::styled(
        fetched,
        Style::default().fg(Color::DarkGray),
    )));
    for issue in &group.issues {
        lines.push(Line::from(Span::styled(
            format!("  {}", issue_text(issue, now_epoch)),
            Style::default().fg(Color::Yellow),
        )));
    }
    lines.push(Line::from(""));
}

fn append_overview_account(
    lines: &mut Vec<Line<'static>>,
    account: &UsageAccount,
    width: usize,
    now_epoch: i64,
) {
    lines.push(Line::from(Span::styled(
        format!("{} · {}", account.provider, account.account),
        Style::default().fg(Color::White),
    )));
    lines.push(Line::from(Span::styled(
        format!("  {}", freshness_age_label(now_epoch, account)),
        Style::default().fg(Color::DarkGray),
    )));
    if let Some(plan) = non_empty_label(account.plan_label.as_ref()) {
        lines.push(Line::from(format!("  Plan {plan}")));
    }
    if let Some(expires_at) = account.credential_expires_at_epoch {
        lines.push(Line::from(format!(
            "  Credential {}",
            credential_expiry_label(now_epoch, expires_at)
        )));
    }
    if account.windows.is_empty() && account.metric_groups.is_empty() {
        lines.push(Line::from(format!("  {}", account.status)));
    } else {
        for window in &account.windows {
            append_overview_window(lines, window, width);
        }
        for group in &account.metric_groups {
            append_overview_group(lines, group, now_epoch);
        }
    }
    append_account_issue_lines(lines, account, now_epoch);
    lines.push(Line::from(""));
}

fn panel(title: &'static str) -> Block<'static> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
}

fn row_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    }
}

/// Meter bar for a known percent. `None` renders no bar: unknown quota
/// must never show an empty (fabricated) meter.
fn meter_line(width: usize, percent: Option<u8>) -> Option<String> {
    let percent = usize::from(percent?.min(100));
    let filled = width.saturating_mul(percent) / 100;
    Some(format!(
        "  {}{}",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled))
    ))
}

/// Meter color from the canonical quota state, never from a local percent
/// threshold. Severity is Rust-owned quota semantics: the projection maps
/// API `Danger`→`Exhausted` and `Warn`→`Warning`, and the Capsule accent
/// reads that same severity — so a renderer-inferred 15/35 split would
/// color the same bucket differently on the two surfaces (S4/S5 parity).
/// Unknown and permission states keep the neutral default: without usable
/// quota the bar usually does not render at all.
fn meter_style(quota_state: UsageQuotaStateV1) -> Style {
    match quota_state {
        UsageQuotaStateV1::Exhausted | UsageQuotaStateV1::Error => Style::default().fg(Color::Red),
        UsageQuotaStateV1::Warning => Style::default().fg(Color::Yellow),
        _ => Style::default().fg(Color::Green),
    }
}

#[cfg(test)]
mod tests;
