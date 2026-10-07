// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Account catalog materialization.

use std::collections::{BTreeMap, BTreeSet};

use std::path::Path;

use jackin_protocol::control::FocusedUsageView;

use super::{
    AccountCatalog, AccountCatalogEntry, AccountLifecycle, AccountProvenance,
    CanonicalAccountIdentity, lifecycle_rank, surface_for_view,
};
use jackin_usage_host_presentation::HostSurfaceId;

/// Durable snapshot reads for catalog materialization.
///
/// The snapshot store is a same-tier sibling, so the T4 host implements
/// this seam by projecting stored rows to their views.
pub trait AccountCatalogStores {
    /// Load every stored account view under `store_path`.
    fn load_stored_views(
        &self,
        store_path: &Path,
        now_epoch: i64,
    ) -> Result<Vec<FocusedUsageView>, String>;
}

/// Current-membership account evidence consumed during materialization.
///
/// Implemented once by the T4 host for discovery descriptors; the
/// catalog only reads these accessors, never discovery types.
pub trait AccountMembershipDescriptor {
    /// Exact provider surface id.
    fn surface_id(&self) -> &str;
    /// Stable canonical account key.
    fn account_key(&self) -> &str;
    /// Authenticated provider label.
    fn account_label(&self) -> &str;
    /// Effective config scopes contributing this account.
    fn provenance(&self) -> &[String];
    /// Canonical identity evidence.
    fn identity(&self) -> CanonicalAccountIdentity;
}

/// Build one catalog by scanning each external source exactly once.
pub fn materialize_account_catalog<M: AccountMembershipDescriptor>(
    live_views: &[(HostSurfaceId, FocusedUsageView, bool)],
    discovered_views: &BTreeMap<(HostSurfaceId, String), FocusedUsageView>,
    discovered_provider_views: &BTreeMap<HostSurfaceId, FocusedUsageView>,
    store_path: &Path,
    membership: Option<&[M]>,
    stores: &dyn AccountCatalogStores,
) -> Result<AccountCatalog, String> {
    let mut catalog = AccountCatalog::default();
    let include_external: BTreeMap<_, _> = live_views
        .iter()
        .map(|(surface, _, include)| (*surface, *include))
        .collect();

    if store_path.exists() {
        for view in stores.load_stored_views(store_path, chrono::Utc::now().timestamp())? {
            let Some(surface) = surface_for_view(&view) else {
                continue;
            };
            if !include_external.get(&surface).copied().unwrap_or(true) {
                continue;
            }
            let identity = membership_identity(membership, surface, &view);
            if membership.is_some() && identity.is_none() {
                continue;
            }
            merge_view(
                &mut catalog,
                surface,
                view,
                if membership.is_some() {
                    AccountLifecycle::Current
                } else {
                    AccountLifecycle::Historical
                },
                AccountProvenance::DurableHistory,
                identity,
            );
        }
    }

    for (surface, view, _) in live_views {
        catalog.provider_states.insert(*surface, view.clone());
        let identity = membership_identity(membership, *surface, view);
        if membership.is_some() && identity.is_none() {
            continue;
        }
        merge_view(
            &mut catalog,
            *surface,
            view.clone(),
            AccountLifecycle::Current,
            AccountProvenance::LiveHost,
            identity,
        );
    }
    catalog.provider_states.extend(
        discovered_provider_views
            .iter()
            .map(|(surface, view)| (*surface, view.clone())),
    );
    if let Some(membership) = membership {
        for ((surface, account_key), view) in discovered_views {
            let Some(account) = membership.iter().find(|account| {
                account.surface_id() == surface.id() && account.account_key() == *account_key
            }) else {
                continue;
            };
            merge_view(
                &mut catalog,
                *surface,
                view.clone(),
                AccountLifecycle::Current,
                AccountProvenance::ConfiguredSource,
                Some(account.identity()),
            );
        }
    }
    if let Some(membership) = membership {
        merge_discovered_placeholders(&mut catalog, membership);
    }
    Ok(catalog)
}

pub(crate) fn membership_identity<M: AccountMembershipDescriptor>(
    membership: Option<&[M]>,
    surface: HostSurfaceId,
    view: &FocusedUsageView,
) -> Option<CanonicalAccountIdentity> {
    let membership = membership?;
    // Existing snapshots retain their pre-V1 routing key during additive
    // migration. Only discovery supplies canonical evidence; display-label
    // comparison never promotes a snapshot into membership.
    let routing_key = CanonicalAccountIdentity::from_view(surface, view)?.account_key();
    membership
        .iter()
        .find(|account| {
            account.surface_id() == surface.id() && account.account_key() == routing_key
        })
        .map(AccountMembershipDescriptor::identity)
}

pub(crate) fn merge_discovered_placeholders<M: AccountMembershipDescriptor>(
    catalog: &mut AccountCatalog,
    membership: &[M],
) {
    for account in membership {
        let surface = account.identity().surface;
        let key = (surface, account.account_key().to_owned());
        if let Some(entry) = catalog.entries.get_mut(&key) {
            entry
                .discovery_provenance
                .extend(account.provenance().iter().cloned());
            entry.lifecycle = AccountLifecycle::Current;
            continue;
        }
        let mut view =
            FocusedUsageView::refreshing(surface.provider_label(), chrono::Utc::now().timestamp());
        view.focused_agent = Some(surface.agent_slug().to_owned());
        view.account.account_label = account.account_label().to_owned();
        view.updated_label = "Not refreshed".to_owned();
        view.last_error = None;
        catalog.entries.insert(
            key,
            AccountCatalogEntry {
                identity: account.identity(),
                account_key: account.account_key().to_owned(),
                account_label: account.account_label().to_owned(),
                username: None,
                plan_label: None,
                provenance: BTreeSet::new(),
                discovery_provenance: account.provenance().iter().cloned().collect(),
                lifecycle: AccountLifecycle::Current,
                fetched_at_epoch: view.fetched_at_epoch,
                view,
            },
        );
    }
}

pub(crate) fn merge_view(
    catalog: &mut AccountCatalog,
    surface: HostSurfaceId,
    view: FocusedUsageView,
    lifecycle: AccountLifecycle,
    provenance: AccountProvenance,
    forced_identity: Option<CanonicalAccountIdentity>,
) {
    let Some(identity) =
        forced_identity.or_else(|| CanonicalAccountIdentity::from_view(surface, &view))
    else {
        return;
    };
    let account_key = identity.account_key();
    let map_key = (surface, account_key.clone());
    let fetched_at_epoch = view.fetched_at_epoch;
    let entry = catalog.entries.entry(map_key).or_insert_with(|| {
        let mut sources = BTreeSet::new();
        sources.insert(provenance);
        AccountCatalogEntry {
            identity: identity.clone(),
            account_key: account_key.clone(),
            account_label: view.account.account_label.trim().to_owned(),
            username: view.account.username.clone(),
            plan_label: view.account.plan_label.clone(),
            provenance: sources,
            discovery_provenance: BTreeSet::new(),
            lifecycle,
            fetched_at_epoch,
            view: view.clone(),
        }
    });
    entry.provenance.insert(provenance);
    let replace = lifecycle_rank(lifecycle) < lifecycle_rank(entry.lifecycle)
        || (lifecycle == entry.lifecycle && fetched_at_epoch >= entry.fetched_at_epoch);
    if replace {
        entry.identity = identity;
        entry.account_label = view.account.account_label.trim().to_owned();
        entry.username = view.account.username.clone();
        entry.plan_label = view.account.plan_label.clone();
        entry.lifecycle = lifecycle;
        entry.fetched_at_epoch = fetched_at_epoch;
        entry.view = view;
    }
}
