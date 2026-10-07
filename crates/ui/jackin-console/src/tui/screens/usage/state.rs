// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage screen state and constructors.

use super::{
    UsageAccount, UsageFilter, UsageMetricGroup, UsageRefreshOutcome, UsageSort, UsageWindow,
    lifecycle_label, well_known_provider_name,
};
use std::time::Instant;

use jackin_protocol::usage_broker::{
    UsageFreshnessPhaseV1, UsageIssueV1, UsageLifecycleV1, UsagePercent,
};

use crate::tui::runtime::BlockingSubscription;

#[derive(Debug, Default)]
pub struct UsageScreenState {
    pub accounts: Vec<UsageAccount>,
    /// Complete broker publication retained through startup, refresh, and navigation.
    pub canonical_projection: Option<jackin_protocol::usage_broker::UsageProjectionV1>,
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
    pub refresh_rx: Option<Box<BlockingSubscription<(u64, UsageRefreshOutcome)>>>,
}

// Manual impls: the in-flight refresh handle carries no value identity.
// Cloning drops it (the worker result is then discarded); equality ignores
// it so tests can compare screen snapshots while a refresh runs. The
// generation counter is value state and survives both.
impl Clone for UsageScreenState {
    fn clone(&self) -> Self {
        Self {
            accounts: self.accounts.clone(),
            canonical_projection: self.canonical_projection.clone(),
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
            && self.canonical_projection == other.canonical_projection
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
    pub fn open_with_snapshot(mut snapshot: Self) -> Self {
        snapshot.refresh_due = true;
        snapshot
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
                provider_issues: projection
                    .providers
                    .iter()
                    .find(|provider| provider.provider_id == unresolved.provider_id)
                    .map_or_else(Vec::new, |provider| provider.issues.clone()),
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
            canonical_projection: Some(projection.clone()),
            generated_at_epoch: Some(projection.generated_at_epoch),
            projection_issues: projection.issues.clone(),
            ..Self::default()
        }
    }
}
