// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `UsageCache` types and methods.

use std::collections::HashMap;

use std::path::PathBuf;

use jackin_protocol::control::{AccountUsageSnapshotView, FocusedUsageView};

use super::{
    MATERIALIZED_USAGE_ACCOUNTS_PATH, account_snapshot_views_from_cache, cached_refreshing_view,
    cached_unavailable_view, cached_usage_for_capability, cached_usage_for_target,
    cached_usage_key_for_target, canonical_usage_cache_key, capability_matches_surface,
    enrich_provider_tabs, mark_active_tab, now_epoch, refresh_cached_updated_label,
    stable_cache_account_label, usage_cache_key_for_broker_account, usage_cache_key_for_view,
    write_materialized_usage_accounts,
};

#[derive(Debug, Clone)]
pub struct UsageCache {
    pub(crate) snapshots: HashMap<String, CachedUsage>,
    /// Destination for accounts.json materialization. Production uses
    /// [`MATERIALIZED_USAGE_ACCOUNTS_PATH`]; benches/tests inject a temp path
    /// via [`UsageCache::set_accounts_materialize_path`].
    pub(crate) accounts_materialize_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct CachedUsage {
    pub(crate) view: FocusedUsageView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageRefreshTarget {
    pub agent: String,
    pub provider: Option<String>,
    /// Exact broker authority for the selected account. Surface labels are
    /// presentation only and must never route a refresh.
    pub capability: jackin_protocol::usage_broker::UsageAccountCapability,
}

impl UsageRefreshTarget {
    pub(crate) fn cache_key(&self) -> String {
        usage_cache_key_for_broker_account(&self.agent, self.provider.as_deref(), &self.capability)
    }
}

impl UsageCache {
    /// Test-only helper: seed a snapshot into the cache. Kept `pub` for the
    /// Capsule daemon tests in a separate crate.
    #[doc(hidden)]
    pub fn insert_snapshot_for_test(
        &mut self,
        agent: &str,
        focused_provider: Option<&str>,
        mut view: FocusedUsageView,
    ) {
        if view.focused_agent.is_none() {
            view.focused_agent = Some(agent.to_owned());
        }
        if view.focused_provider.is_none() {
            view.focused_provider = focused_provider.map(str::to_owned);
        }
        let cache_key = stable_cache_account_label(&view.account.account_label)
            .map(|_| usage_cache_key_for_view(agent, focused_provider, &view))
            .or_else(|| cached_usage_key_for_target(&self.snapshots, agent, focused_provider))
            .unwrap_or_else(|| canonical_usage_cache_key(agent, focused_provider));
        self.snapshots.insert(cache_key, CachedUsage { view });
    }

    /// Test-only helper for an exact account snapshot. Production broker
    /// adoption always uses this same capability-qualified key.
    #[doc(hidden)]
    pub fn insert_snapshot_for_capability_for_test(
        &mut self,
        agent: &str,
        focused_provider: Option<&str>,
        capability: &jackin_protocol::usage_broker::UsageAccountCapability,
        mut view: FocusedUsageView,
    ) {
        if view.focused_agent.is_none() {
            view.focused_agent = Some(agent.to_owned());
        }
        if view.focused_provider.is_none() {
            view.focused_provider = focused_provider.map(str::to_owned);
        }
        let cache_key = usage_cache_key_for_broker_account(agent, focused_provider, capability);
        self.snapshots.insert(cache_key, CachedUsage { view });
    }

    /// Bench/test helper: write materialized accounts to `path` instead of the
    /// container path. Cross-crate like `insert_snapshot_for_test`.
    #[doc(hidden)]
    pub fn set_accounts_materialize_path(&mut self, path: PathBuf) {
        self.accounts_materialize_path = path;
    }

    /// Bench/test entry: materialize the cache to the configured path.
    /// Production refresh calls the same body via [`Self::materialize_accounts`].
    #[doc(hidden)]
    pub fn materialize_accounts_for_bench(&self, generated_at_epoch: i64) -> Result<(), String> {
        self.materialize_accounts(generated_at_epoch)
    }

    pub fn focused_status_bar_label(
        &self,
        focused_agent: Option<&str>,
        focused_provider: Option<&str>,
    ) -> Option<String> {
        let agent = focused_agent?;
        // Label-only fast path: the status bar needs just `status_bar_label`, which
        // `cached_focused_usage_view`'s clone + enrich/mark-active never touch. Read
        // it straight from the stored view instead of cloning the whole snapshot.
        if let Some(cached) = cached_usage_for_target(&self.snapshots, agent, focused_provider) {
            return Some(cached.view.status_bar_label.clone());
        }
        // A focused agent with no snapshot yet is mid-load — show `refreshing`
        // (clickable to force a load), never blank or a stale headline. The
        // segment is hidden only when there is no focused agent at all (the
        // `focused_agent?` above returns `None` → caller renders nothing).
        Some("refreshing".to_owned())
    }

    /// Exact-account status-bar lookup used by Capsule sessions. A missing
    /// capability is intentionally treated as not yet loaded; it must never
    /// fall back to another account on the same provider surface.
    pub fn focused_status_bar_label_for_capability(
        &self,
        focused_agent: Option<&str>,
        focused_provider: Option<&str>,
        capability: Option<&jackin_protocol::usage_broker::UsageAccountCapability>,
    ) -> Option<String> {
        let agent = focused_agent?;
        let Some(capability) = capability else {
            return Some("refreshing".to_owned());
        };
        if !capability_matches_surface(agent, focused_provider, capability) {
            return Some("usage unavailable".to_owned());
        }
        Some(
            cached_usage_for_capability(&self.snapshots, agent, focused_provider, capability)
                .map_or_else(
                    || "refreshing".to_owned(),
                    |cached| cached.view.status_bar_label.clone(),
                ),
        )
    }

    pub fn account_snapshot_views(&self) -> Vec<AccountUsageSnapshotView> {
        account_snapshot_views_from_cache(&self.snapshots)
    }

    pub fn focused_snapshot(
        &mut self,
        focused_agent: Option<&str>,
        focused_provider: Option<&str>,
    ) -> FocusedUsageView {
        let Some(agent) = focused_agent else {
            if let Some(provider) = focused_provider {
                return cached_unavailable_view("usage", Some(provider), now_epoch());
            }
            return FocusedUsageView::unavailable("no focused agent session", now_epoch());
        };
        let now = now_epoch();
        if let Some(view) = self.cached_focused_usage_view(agent, focused_provider) {
            return view;
        }
        // Agent is focused but no snapshot is cached yet: the agent has started
        // and the fetch is in flight — an honest "refreshing" state, not the
        // "usage unavailable" we reserve for a genuine absence.
        cached_refreshing_view(agent, focused_provider, now)
    }

    /// Exact-account focused snapshot. Surface-only cache selection is not
    /// acceptable for a Capsule with duplicate provider accounts.
    pub fn focused_snapshot_for_capability(
        &mut self,
        focused_agent: Option<&str>,
        focused_provider: Option<&str>,
        capability: Option<&jackin_protocol::usage_broker::UsageAccountCapability>,
    ) -> FocusedUsageView {
        let Some(agent) = focused_agent else {
            if let Some(provider) = focused_provider {
                return cached_unavailable_view("usage", Some(provider), now_epoch());
            }
            return FocusedUsageView::unavailable("no focused agent session", now_epoch());
        };
        let now = now_epoch();
        let Some(capability) = capability else {
            return cached_refreshing_view(agent, focused_provider, now);
        };
        if !capability_matches_surface(agent, focused_provider, capability) {
            return cached_unavailable_view(agent, focused_provider, now);
        }
        if let Some(view) =
            self.cached_focused_usage_view_for_capability(agent, focused_provider, capability)
        {
            return view;
        }
        cached_refreshing_view(agent, focused_provider, now)
    }

    pub(crate) fn cached_focused_usage_view(
        &self,
        agent: &str,
        focused_provider: Option<&str>,
    ) -> Option<FocusedUsageView> {
        let mut view = cached_usage_for_target(&self.snapshots, agent, focused_provider)
            .map(|cached| cached.view.clone())?;
        refresh_cached_updated_label(&mut view, now_epoch());
        if view.focused_agent.is_none() {
            view.focused_agent = Some(agent.to_owned());
        }
        if view.focused_provider.is_none() {
            view.focused_provider = focused_provider.map(str::to_owned);
        }
        enrich_provider_tabs(&mut view, &self.snapshots);
        mark_active_tab(&mut view);
        Some(view)
    }

    pub(crate) fn cached_focused_usage_view_for_capability(
        &self,
        agent: &str,
        focused_provider: Option<&str>,
        capability: &jackin_protocol::usage_broker::UsageAccountCapability,
    ) -> Option<FocusedUsageView> {
        let mut view =
            cached_usage_for_capability(&self.snapshots, agent, focused_provider, capability)
                .map(|cached| cached.view.clone())?;
        refresh_cached_updated_label(&mut view, now_epoch());
        if view.focused_agent.is_none() {
            view.focused_agent = Some(agent.to_owned());
        }
        if view.focused_provider.is_none() {
            view.focused_provider = focused_provider.map(str::to_owned);
        }
        enrich_provider_tabs(&mut view, &self.snapshots);
        mark_active_tab(&mut view);
        Some(view)
    }

    pub(crate) fn materialize_accounts(&self, generated_at_epoch: i64) -> Result<(), String> {
        let snapshots: Vec<&FocusedUsageView> =
            self.snapshots.values().map(|cached| &cached.view).collect();
        write_materialized_usage_accounts(
            &self.accounts_materialize_path,
            generated_at_epoch,
            &snapshots,
        )
    }
}

impl Default for UsageCache {
    fn default() -> Self {
        Self {
            snapshots: HashMap::new(),
            accounts_materialize_path: PathBuf::from(MATERIALIZED_USAGE_ACCOUNTS_PATH),
        }
    }
}
