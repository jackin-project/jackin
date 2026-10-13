// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `HostUsageRuntime` account selection and catalog.

use super::{
    HOST_USAGE_STATE_REL, HostAccountCatalogStores, HostAccountDescriptor,
    HostSelectedAccountRoute, HostSurfaceId, HostUsageRuntime, SELECTED_ACCOUNT_UNAVAILABLE_NOTICE,
    account_descriptor, accounts, host_snapshot_store_path, selected_account_unavailable_view,
};

use jackin_protocol::control::FocusedUsageView;

impl HostUsageRuntime {
    /// List known accounts for one surface (or all surfaces when `None`).
    ///
    /// Sources: current broker discovery and durable broker history.
    pub fn list_accounts(
        &mut self,
        surface_id: Option<&str>,
    ) -> Result<Vec<HostAccountDescriptor>, String> {
        self.require_open()?;
        let surfaces: Vec<HostSurfaceId> = match surface_id {
            Some(id) => {
                let surface =
                    HostSurfaceId::from_id(id).ok_or_else(|| format!("unknown surface: {id}"))?;
                vec![surface]
            }
            None => HostSurfaceId::DESKTOP_PROVIDER_ORDER.to_vec(),
        };
        let catalog = self.materialize_account_catalog()?;
        self.reconcile_selected_accounts(&catalog, &surfaces)?;
        let now = chrono::Utc::now().timestamp();
        let prefs = self.format_prefs;
        let mut out = Vec::new();
        for surface in surfaces {
            let selected = self.selected_accounts.get(surface.id()).map(String::as_str);
            for entry in catalog.entries_for_surface(surface) {
                out.push(account_descriptor(
                    surface,
                    entry,
                    selected == Some(entry.account_key.as_str()),
                    now,
                    prefs,
                ));
            }
        }
        Ok(out)
    }

    /// Select which account drives detail/snapshot for a surface (persisted).
    pub fn set_selected_account(
        &mut self,
        surface_id: &str,
        account_key: &str,
    ) -> Result<(), String> {
        self.require_open()?;
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        if account_key.is_empty() {
            self.selected_accounts.remove(surface.id());
        } else {
            let catalog = self.materialize_account_catalog()?;
            if catalog.entry(surface, account_key).is_none() {
                return Err(format!(
                    "account key does not belong to surface {surface_id}"
                ));
            }
            self.selected_accounts
                .insert(surface.id().to_owned(), account_key.to_owned());
        }
        if let Some(dir) = &self.data_dir {
            let path = accounts::selected_accounts_path(dir, HOST_USAGE_STATE_REL);
            accounts::save_selected_accounts(&path, &self.selected_accounts)?;
        }
        self.push_event(
            "account_selected",
            Some(surface.id()),
            Some(account_key.to_owned()),
        );
        Ok(())
    }

    pub(crate) fn materialize_account_catalog(
        &mut self,
    ) -> Result<accounts::AccountCatalog, String> {
        let mut live_views = Vec::with_capacity(HostSurfaceId::ALL.len());
        for surface in HostSurfaceId::ALL.iter().copied() {
            let view = self
                .cache
                .focused_snapshot(Some(surface.agent_slug()), surface.provider_label());
            live_views.push((surface, view, true));
        }
        let store_path = self
            .data_dir
            .as_ref()
            .map(|dir| host_snapshot_store_path(dir))
            .unwrap_or_default();
        accounts::materialize_account_catalog(
            &live_views,
            &self.discovered_views,
            &self.discovered_provider_views,
            &store_path,
            self.discovery
                .as_ref()
                .map(|discovery| discovery.accounts.as_slice()),
            &HostAccountCatalogStores,
        )
    }

    pub(crate) fn reconcile_selected_accounts(
        &mut self,
        catalog: &accounts::AccountCatalog,
        surfaces: &[HostSurfaceId],
    ) -> Result<(), String> {
        let before = self.selected_accounts.clone();
        // Persisted selection is operator intent, including while its account
        // is missing. A sibling must never become an implicit replacement.
        self.selected_accounts
            .retain(|surface_id, _| HostSurfaceId::from_id(surface_id).is_some());
        for surface in surfaces {
            if !self.selected_accounts.contains_key(surface.id())
                && let Some(key) = catalog.preferred_current_key(*surface)
            {
                self.selected_accounts.insert(surface.id().to_owned(), key);
            }
        }
        if self.selected_accounts != before
            && let Some(data_dir) = &self.data_dir
        {
            accounts::save_selected_accounts(
                &accounts::selected_accounts_path(data_dir, HOST_USAGE_STATE_REL),
                &self.selected_accounts,
            )?;
        }
        Ok(())
    }

    pub(crate) fn selected_account_missing(
        &self,
        catalog: &accounts::AccountCatalog,
        surface: HostSurfaceId,
    ) -> bool {
        self.selected_accounts
            .get(surface.id())
            .is_some_and(|key| catalog.entry(surface, key).is_none())
    }

    pub(crate) fn selected_view_for_catalog(
        &self,
        catalog: &accounts::AccountCatalog,
        surface: HostSurfaceId,
    ) -> Option<FocusedUsageView> {
        self.selected_route_and_view_for_catalog(catalog, surface).1
    }

    pub(crate) fn selected_route_and_view_for_catalog(
        &self,
        catalog: &accounts::AccountCatalog,
        surface: HostSurfaceId,
    ) -> (HostSelectedAccountRoute, Option<FocusedUsageView>) {
        let Some(key) = self.selected_accounts.get(surface.id()) else {
            return (
                HostSelectedAccountRoute::Unselected,
                catalog.provider_state(surface).cloned(),
            );
        };
        if let Some(entry) = catalog.entry(surface, key) {
            return (
                HostSelectedAccountRoute::Available {
                    account_key: key.clone(),
                },
                Some(entry.view.clone()),
            );
        }
        if self.discovery.is_none()
            && catalog.entries_for_surface(surface).is_empty()
            && catalog
                .provider_state(surface)
                .is_some_and(FocusedUsageView::is_refreshing_placeholder)
        {
            // Before membership is known, retain honest loading without
            // borrowing another account's identity or quota.
            return (
                HostSelectedAccountRoute::Resolving {
                    account_key: key.clone(),
                },
                catalog.provider_state(surface).cloned(),
            );
        }
        (
            HostSelectedAccountRoute::Unavailable {
                account_key: key.clone(),
                notice: SELECTED_ACCOUNT_UNAVAILABLE_NOTICE,
            },
            Some(selected_account_unavailable_view(surface)),
        )
    }
}
