// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Catalog reconciliation methods.

use std::collections::{BTreeMap, BTreeSet};

use std::sync::Arc;

use jackin_protocol::usage_broker::{UsageCatalogEntry, UsageCoordinationError};

use super::{
    CatalogAccountPreimage, CatalogTransaction, StateStoreError, UsageCoordinator,
    catalog_entries_from_map, catalog_purge_set, first_catalog_rollback_error,
    preserve_catalog_error, reconcile_executor_catalog, reset_entry, restore_catalog_preimages,
    revoke_entry, state_error, unavailable_error, validate_catalog_entries,
};

impl UsageCoordinator {
    /// Reconcile the live broker catalog with the last durable catalog.
    /// Removed capabilities are fenced in memory and purged from durable
    /// account state. Revision changes fence old work and discard its
    /// materialized result before the capability can refresh again.
    pub fn reconcile_catalog(
        &self,
        entries: impl IntoIterator<Item = UsageCatalogEntry>,
        now_epoch: i64,
    ) -> Result<(), UsageCoordinationError> {
        self.reconcile_catalog_transaction_with_revision(None, entries, now_epoch)
            .map(|_| ())
    }

    /// Apply one catalog rotation with the caller's content-derived catalog
    /// revision. Broker processes use this stronger boundary so executor
    /// discovery can reject a caller/service catalog mismatch before state is
    /// changed.
    pub fn reconcile_catalog_transaction_with_revision(
        &self,
        catalog_revision: Option<&str>,
        entries: impl IntoIterator<Item = UsageCatalogEntry>,
        now_epoch: i64,
    ) -> Result<CatalogTransaction, UsageCoordinationError> {
        let entries = entries.into_iter().collect::<Vec<_>>();
        let _catalog_lifecycle = self
            .shared
            .catalog_lifecycle
            .lock()
            .map_err(|_| unavailable_error())?;
        let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        validate_catalog_entries(&entries)?;
        if let Some(catalog_revision) = catalog_revision {
            self.shared
                .executor
                .validate_catalog_revision(catalog_revision, &entries)?;
        } else {
            self.shared.executor.validate_catalog(&entries)?;
        }
        let previous_state = state.clone();
        let previous = state.catalog.clone().unwrap_or_else(|| {
            state
                .accounts
                .iter()
                .filter_map(|(capability, entry)| {
                    entry
                        .catalog_revision
                        .clone()
                        .map(|revision| (capability.clone(), revision))
                })
                .collect()
        });
        let next = entries
            .iter()
            .cloned()
            .map(|entry| (entry.capability, entry.revision))
            .collect::<BTreeMap<_, _>>();
        let purge = catalog_purge_set(&state, &previous, &next);
        let preimages = purge
            .iter()
            .map(|capability| {
                self.shared
                    .store
                    .load(capability, now_epoch)
                    .map(|envelope| {
                        (
                            capability.clone(),
                            envelope.map_or(CatalogAccountPreimage::Missing, |envelope| {
                                CatalogAccountPreimage::Present(Box::new(envelope))
                            }),
                        )
                    })
                    .or_else(|error| match error {
                        StateStoreError::Corrupt => {
                            Ok((capability.clone(), CatalogAccountPreimage::Corrupt))
                        }
                        StateStoreError::Unavailable => Err(state_error(error)),
                    })
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        if let Err(error) = reconcile_executor_catalog(&self.shared, catalog_revision, &entries) {
            let rollback = reconcile_executor_catalog(
                &self.shared,
                previous_state.catalog_revision.as_deref(),
                &catalog_entries_from_map(&previous),
            );
            return Err(preserve_catalog_error(error, rollback));
        }

        let mut completed_purges = BTreeSet::new();
        for capability in &purge {
            let result: Result<(), StateStoreError> = match preimages.get(capability) {
                Some(CatalogAccountPreimage::Corrupt) => self.shared.store.quarantine(capability),
                Some(CatalogAccountPreimage::Missing | CatalogAccountPreimage::Present(_)) => {
                    self.shared.store.purge(capability)
                }
                None => Err(StateStoreError::Unavailable),
            };
            if let Err(error) = result.map_err(state_error) {
                let durable = restore_catalog_preimages(
                    &self.shared,
                    &preimages,
                    completed_purges,
                    now_epoch,
                )
                .map_err(state_error);
                let executor = reconcile_executor_catalog(
                    &self.shared,
                    previous_state.catalog_revision.as_deref(),
                    &catalog_entries_from_map(&previous),
                );
                return Err(preserve_catalog_error(
                    error,
                    first_catalog_rollback_error(durable, executor),
                ));
            }
            completed_purges.insert(capability.clone());
        }

        let account_capabilities = state.accounts.keys().cloned().collect::<Vec<_>>();
        for capability in account_capabilities {
            let Some(entry) = state.accounts.get_mut(&capability) else {
                continue;
            };
            match next.get(&capability) {
                None => {
                    revoke_entry(entry, now_epoch);
                    state.blocked.remove(&capability);
                }
                Some(revision)
                    if entry.revoked || entry.catalog_revision.as_ref() != Some(revision) =>
                {
                    reset_entry(entry, now_epoch, revision.clone());
                    state.blocked.remove(&capability);
                }
                Some(_) => {}
            }
        }
        state.catalog = Some(next);
        state.catalog_revision = catalog_revision.map(str::to_owned);
        let previous_catalog_revision = previous_state.catalog_revision.clone();
        self.shared.changed.notify_all();
        Ok(CatalogTransaction {
            shared: Arc::clone(&self.shared),
            previous_state,
            preimages,
            previous_entries: catalog_entries_from_map(&previous),
            previous_catalog_revision,
        })
    }
}
