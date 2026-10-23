// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{
    CachedUsage, FocusedUsageView, UsageCache, UsageRefreshTarget, UsageSnapshotStatus,
    capability_matches_surface, now_epoch, refresh_cached_updated_label,
    usage_cache_key_for_broker_account,
};
use super::{
    enrich_provider_tabs, mark_active_tab, refresh_failed_view_presentation, usage_tab_id_for_view,
};

impl UsageCache {
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
        // The exact target/state match and surface check above authenticate this
        // selector for the adopted view; discard any provider-supplied tab set.
        view.account.account_capability = Some(target.capability.clone());
        view.tabs.clear();
        self.snapshots.insert(
            usage_cache_key_for_broker_account(
                &target.agent,
                target.provider.as_deref(),
                &state.capability,
            ),
            CachedUsage { view },
        );
    }

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
            view.account.account_capability = Some(target.capability.clone());
            CachedUsage { view }
        });
        if cached.view.account.account_capability.as_ref() != Some(&target.capability) {
            let mut view = FocusedUsageView::unavailable(&error.message, now_epoch());
            view.focused_agent = Some(target.agent.clone());
            view.focused_provider = target.provider.clone();
            view.account.account_capability = Some(target.capability.clone());
            cached.view = view;
        }
        cached.view.tabs.clear();
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
    pub fn focused_snapshot_for_account_id(&self, tab_id: &str) -> Option<FocusedUsageView> {
        let bound = self
            .snapshots
            .values()
            .filter(|cached| {
                cached
                    .view
                    .account
                    .account_capability
                    .as_ref()
                    .is_some_and(|capability| capability.opaque_identity_key() == tab_id)
            })
            .collect::<Vec<_>>();
        let cached = if let Some(first) = bound.first() {
            let capability = first.view.account.account_capability.as_ref()?;
            if bound
                .iter()
                .any(|cached| cached.view.account.account_capability.as_ref() != Some(capability))
            {
                return None;
            }
            bound
                .into_iter()
                .max_by_key(|cached| cached.view.fetched_at_epoch)?
        } else {
            let mut unbound = self.snapshots.values().filter(|cached| {
                cached.view.account.account_capability.is_none()
                    && usage_tab_id_for_view(&cached.view) == tab_id
            });
            let cached = unbound.next()?;
            if unbound.next().is_some() {
                return None;
            }
            cached
        };
        let mut view = cached.view.clone();
        refresh_cached_updated_label(&mut view, now_epoch());
        enrich_provider_tabs(&mut view, &self.snapshots);
        mark_active_tab(&mut view);
        Some(view)
    }

    pub fn broker_capability_for_tab_id(
        &self,
        tab_id: &str,
    ) -> Option<jackin_protocol::usage_broker::UsageAccountCapability> {
        let mut selected = None;
        for capability in self
            .snapshots
            .values()
            .filter_map(|cached| cached.view.account.account_capability.as_ref())
        {
            if capability.opaque_identity_key() != tab_id {
                continue;
            }
            if selected
                .as_ref()
                .is_some_and(|previous| previous != capability)
            {
                return None;
            }
            selected = Some(capability.clone());
        }
        selected
    }
}
