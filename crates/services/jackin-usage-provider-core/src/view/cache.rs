// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `UsageCache` view methods.

use super::super::{
    CachedUsage, FocusedUsageView, UsageCache, UsageRefreshTarget, UsageSnapshotStatus,
    capability_matches_surface, now_epoch, refresh_cached_updated_label,
    usage_cache_key_for_broker_account,
};
use super::{
    broker_account_id_from_cache_key, enrich_provider_tabs, mark_active_tab,
    refresh_failed_view_presentation, usage_account_tab_id,
};

impl UsageCache {
    /// Adopt one host-broker generation without executing provider work locally.
    pub fn adopt_broker_generation(
        &mut self,
        target: &UsageRefreshTarget,
        state: &jackin_protocol::usage_broker::UsageGenerationView,
    ) {
        if target.capability != state.capability
            || !capability_matches_surface(
                &target.agent,
                target.provider.as_deref(),
                &state.capability,
            )
        {
            return;
        }
        let mut view = state.snapshot.clone().unwrap_or_else(|| {
            if state.phase.is_active() {
                FocusedUsageView::refreshing(target.provider.as_deref(), now_epoch())
            } else {
                FocusedUsageView::unavailable(
                    state
                        .error
                        .as_ref()
                        .map_or("usage coordinator unavailable", |error| {
                            error.message.as_str()
                        }),
                    now_epoch(),
                )
            }
        });
        if let Some(error) = &state.error {
            view.last_error = Some(error.message.clone());
            view.status = if view.buckets.is_empty() {
                UsageSnapshotStatus::Error
            } else {
                UsageSnapshotStatus::Stale
            };
        }
        if view.focused_agent.is_none() {
            view.focused_agent = Some(target.agent.clone());
        }
        if view.focused_provider.is_none() {
            view.focused_provider = target.provider.clone();
        }
        if state.error.is_some() {
            refresh_failed_view_presentation(&mut view);
        }
        self.snapshots.insert(
            usage_cache_key_for_broker_account(
                &target.agent,
                target.provider.as_deref(),
                &state.capability,
            ),
            CachedUsage { view },
        );
    }

    /// Preserve last-good quota while surfacing a typed relay/broker failure.
    pub fn adopt_broker_error(
        &mut self,
        target: &UsageRefreshTarget,
        error: &jackin_protocol::usage_broker::UsageCoordinationError,
    ) {
        if !capability_matches_surface(
            &target.agent,
            target.provider.as_deref(),
            &target.capability,
        ) {
            return;
        }
        let cache_key = target.cache_key();
        let cached = self.snapshots.entry(cache_key).or_insert_with(|| {
            let mut view = FocusedUsageView::unavailable(&error.message, now_epoch());
            view.focused_agent = Some(target.agent.clone());
            view.focused_provider = target.provider.clone();
            CachedUsage { view }
        });
        cached.view.last_error = Some(error.message.clone());
        cached.view.status = if cached.view.buckets.is_empty() {
            UsageSnapshotStatus::Error
        } else {
            UsageSnapshotStatus::Stale
        };
        refresh_failed_view_presentation(&mut cached.view);
    }
}

impl UsageCache {
    /// Focused snapshot for an exact canonical account id (tab selection).
    /// `None` when no admitted snapshot carries the id; the caller falls back
    /// to label resolution only for empty ids (old payloads).
    pub fn focused_snapshot_for_account_id(&self, account_id: &str) -> Option<FocusedUsageView> {
        let mut view = self
            .snapshots
            .values()
            .filter(|cached| {
                usage_account_tab_id(
                    &cached.view.account.provider_label,
                    &cached.view.account.account_label,
                ) == account_id
            })
            .max_by_key(|cached| cached.view.fetched_at_epoch)
            .map(|cached| cached.view.clone())?;
        refresh_cached_updated_label(&mut view, now_epoch());
        enrich_provider_tabs(&mut view, &self.snapshots);
        mark_active_tab(&mut view);
        Some(view)
    }

    /// Broker-namespace account id behind an exact tab id, recovered from the
    /// owning cache entry's broker key. `None` for unknown ids and for
    /// non-broker entries (legacy keys carry no capability).
    pub fn broker_account_id_for_tab_id(&self, tab_id: &str) -> Option<String> {
        self.snapshots
            .iter()
            .filter(|(_, cached)| {
                usage_account_tab_id(
                    &cached.view.account.provider_label,
                    &cached.view.account.account_label,
                ) == tab_id
            })
            .filter_map(|(key, cached)| {
                broker_account_id_from_cache_key(key).map(|id| (cached.view.fetched_at_epoch, id))
            })
            .max_by_key(|(fetched_at_epoch, _)| *fetched_at_epoch)
            .map(|(_, id)| id)
    }
}
