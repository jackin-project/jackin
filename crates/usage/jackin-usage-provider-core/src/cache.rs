// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
    pub snapshots: HashMap<String, CachedUsage>,
    pub accounts_materialize_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CachedUsage {
    pub view: FocusedUsageView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageRefreshTarget {
    pub agent: String,
    pub provider: Option<String>,
    pub capability: jackin_protocol::usage_broker::UsageAccountCapability,
}

impl UsageRefreshTarget {
    pub(crate) fn cache_key(&self) -> String {
        usage_cache_key_for_broker_account(&self.agent, self.provider.as_deref(), &self.capability)
    }
}

impl UsageCache {
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
        view.account.account_capability = Some(capability.clone());
        let cache_key = usage_cache_key_for_broker_account(agent, focused_provider, capability);
        self.snapshots.insert(cache_key, CachedUsage { view });
    }

    #[doc(hidden)]
    pub fn set_accounts_materialize_path(&mut self, path: PathBuf) {
        self.accounts_materialize_path = path;
    }

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
        if let Some(cached) = cached_usage_for_target(&self.snapshots, agent, focused_provider) {
            return Some(cached.view.status_bar_label.clone());
        }
        Some("refreshing".to_owned())
    }

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
        let Some(cached) =
            cached_usage_for_capability(&self.snapshots, agent, focused_provider, capability)
        else {
            return Some("refreshing".to_owned());
        };
        Some(
            if cached.view.account.account_capability.as_ref() == Some(capability) {
                cached.view.status_bar_label.clone()
            } else {
                "refreshing".to_owned()
            },
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
        cached_refreshing_view(agent, focused_provider, now)
    }

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
        let cached =
            cached_usage_for_capability(&self.snapshots, agent, focused_provider, capability)?;
        if cached.view.account.account_capability.as_ref() != Some(capability) {
            return None;
        }
        let mut view = cached.view.clone();
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
