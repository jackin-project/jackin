// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Catalog reconciliation methods.

use std::collections::{BTreeMap, BTreeSet};

use std::sync::Arc;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageRefreshPhase,
};

use super::{
    CatalogAccountPreimage, CatalogTransaction, CoordinatorState, Shared, StateStoreError,
    UsageCoordinator, catalog_entries_from_map, catalog_purge_set, cooldown_tombstone,
    first_catalog_rollback_error, pending_attempt_envelope, preserve_catalog_error,
    reconcile_executor_catalog, reset_entry, reset_envelope, restore_catalog_preimages,
    revoke_entry, revoke_envelope, state_error, unavailable_error, validate_catalog_entries,
};

fn persist_reset_envelopes(
    shared: &Shared,
    state: &CoordinatorState,
    reset_capabilities: &BTreeSet<UsageAccountCapability>,
    preimages: &BTreeMap<UsageAccountCapability, CatalogAccountPreimage>,
    clock_sample: super::ClockSample,
) -> Result<(), StateStoreError> {
    for capability in reset_capabilities {
        let pending_attempt = has_pending_provider_attempt(state, preimages, capability);
        let envelope = state
            .accounts
            .get(capability)
            .map(|entry| entry.envelope.clone())
            .or_else(|| match preimages.get(capability) {
                Some(CatalogAccountPreimage::Present(envelope)) => {
                    let mut envelope = (**envelope).clone();
                    reset_envelope(&mut envelope);
                    Some(envelope)
                }
                Some(CatalogAccountPreimage::Missing | CatalogAccountPreimage::Corrupt) | None => {
                    None
                }
            });
        if let Some(envelope) = envelope {
            let envelope = if pending_attempt {
                pending_attempt_envelope(&envelope, clock_sample.ceil_epoch())
            } else {
                envelope
            };
            let persist_epoch = clock_sample
                .ceil_epoch()
                .max(envelope.provider_invoked_at_epoch.unwrap_or(0));
            shared.store.store(&envelope, persist_epoch)?;
        }
    }
    Ok(())
}

fn has_pending_provider_attempt(
    state: &CoordinatorState,
    preimages: &BTreeMap<UsageAccountCapability, CatalogAccountPreimage>,
    capability: &UsageAccountCapability,
) -> bool {
    state
        .accounts
        .get(capability)
        .is_some_and(|entry| entry.pending_provider_generation.is_some())
        || preimages.get(capability).is_some_and(|preimage| {
            matches!(preimage, CatalogAccountPreimage::Present(envelope)
                if envelope.phase == UsageRefreshPhase::Updating)
        })
}

fn persist_cooldown_tombstones(
    shared: &Shared,
    state: &CoordinatorState,
    removed_capabilities: &BTreeSet<UsageAccountCapability>,
    preimages: &BTreeMap<UsageAccountCapability, CatalogAccountPreimage>,
    now_epoch: i64,
    clock_sample: super::ClockSample,
) -> Result<(), StateStoreError> {
    for capability in removed_capabilities {
        let already_revoked = state
            .accounts
            .get(capability)
            .is_some_and(|entry| entry.revoked);
        let pending_attempt = has_pending_provider_attempt(state, preimages, capability);
        let Some(mut envelope) = state
            .accounts
            .get(capability)
            .map(|entry| entry.envelope.clone())
            .or_else(|| match preimages.get(capability) {
                Some(CatalogAccountPreimage::Present(envelope)) => Some((**envelope).clone()),
                Some(CatalogAccountPreimage::Missing | CatalogAccountPreimage::Corrupt) | None => {
                    None
                }
            })
        else {
            continue;
        };
        if !already_revoked {
            revoke_envelope(&mut envelope, now_epoch, clock_sample);
        }
        let persist_epoch = clock_sample
            .ceil_epoch()
            .max(envelope.provider_invoked_at_epoch.unwrap_or(0));
        if let Some(tombstone) = cooldown_tombstone(&envelope, persist_epoch, pending_attempt) {
            shared.store.store(&tombstone, persist_epoch)?;
        }
    }
    Ok(())
}

fn rollback_catalog_reconciliation(
    shared: &Arc<Shared>,
    state: &mut CoordinatorState,
    previous_state: &CoordinatorState,
    previous: &BTreeMap<UsageAccountCapability, String>,
    preimages: &BTreeMap<UsageAccountCapability, CatalogAccountPreimage>,
    completed_purges: BTreeSet<UsageAccountCapability>,
    now_epoch: i64,
) -> Result<(), UsageCoordinationError> {
    let durable = restore_catalog_preimages(shared, preimages, completed_purges, now_epoch)
        .map_err(state_error);
    let executor = reconcile_executor_catalog(
        shared,
        previous_state.catalog_revision.as_deref(),
        &catalog_entries_from_map(previous),
    );
    *state = previous_state.clone();
    first_catalog_rollback_error(durable, executor)
}

impl UsageCoordinator {
    /// Reconcile the live broker catalog with the last durable catalog.
    /// Removed capabilities are fenced in memory. A minimal durable cooldown
    /// tombstone survives removal so re-adding the same canonical account
    /// cannot bypass its provider or retry deadline. Revision changes discard
    /// materialized results before the capability can refresh again.
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
        let removed_capabilities = purge
            .iter()
            .filter(|capability| !next.contains_key(*capability))
            .cloned()
            .collect::<BTreeSet<_>>();
        let reset_capabilities = purge
            .iter()
            .filter(|capability| next.contains_key(*capability))
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut preimages = purge
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
                        StateStoreError::Unavailable
                        | StateStoreError::SchemaMigrationRequired { .. } => {
                            Err(state_error(error))
                        }
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

        let revocation_sample = self.shared.clock.sample(now_epoch);
        let mut completed_purges = BTreeSet::new();
        let mut preserved_pending_markers = BTreeSet::new();
        for capability in &purge {
            if !has_pending_provider_attempt(&state, &preimages, capability) {
                continue;
            }
            let Some(mut envelope) = state
                .accounts
                .get(capability)
                .map(|entry| entry.envelope.clone())
                .or_else(|| match preimages.get(capability) {
                    Some(CatalogAccountPreimage::Present(envelope)) => Some((**envelope).clone()),
                    Some(CatalogAccountPreimage::Missing | CatalogAccountPreimage::Corrupt)
                    | None => None,
                })
            else {
                continue;
            };
            let persist_epoch = revocation_sample
                .ceil_epoch()
                .max(envelope.provider_invoked_at_epoch.unwrap_or(0));
            let is_removed = removed_capabilities.contains(capability);
            let marker = if is_removed {
                let already_revoked = state
                    .accounts
                    .get(capability)
                    .is_some_and(|entry| entry.revoked);
                if !already_revoked {
                    revoke_envelope(&mut envelope, now_epoch, revocation_sample);
                }
                cooldown_tombstone(&envelope, persist_epoch, true)
            } else {
                // Revision changes also purge the old durable record. Install
                // the post-reset active marker first so a crash in that gap
                // recovers the in-flight attempt instead of permitting a
                // second provider call.
                reset_envelope(&mut envelope);
                Some(pending_attempt_envelope(&envelope, persist_epoch))
            };
            let Some(marker) = marker else {
                continue;
            };

            // Replace the durable record with a result-free active marker
            // before deleting anything. A process crash during catalog
            // rotation then recovers the attempt instead of losing its floor.
            // If no durable preimage existed, keep this sanitized marker as
            // the rollback image too. A later failure must not purge the
            // only recovery fence that was successfully written.
            if matches!(
                preimages.get(capability),
                Some(CatalogAccountPreimage::Missing)
            ) {
                preimages.insert(
                    capability.clone(),
                    CatalogAccountPreimage::Present(Box::new(marker.clone())),
                );
            }
            completed_purges.insert(capability.clone());
            if let Err(error) = self.shared.store.store(&marker, persist_epoch) {
                let primary = state_error(error);
                let rollback = rollback_catalog_reconciliation(
                    &self.shared,
                    &mut state,
                    &previous_state,
                    &previous,
                    &preimages,
                    completed_purges.clone(),
                    now_epoch,
                );
                return Err(preserve_catalog_error(primary, rollback));
            }
            preserved_pending_markers.insert(capability.clone());
        }
        for capability in &purge {
            if preserved_pending_markers.contains(capability) {
                continue;
            }
            // A store can report failure after the unlink/quarantine reached
            // disk. Restore this preimage on every failure path, not only
            // after a fully successful purge return.
            completed_purges.insert(capability.clone());
            let result: Result<(), StateStoreError> = match preimages.get(capability) {
                Some(CatalogAccountPreimage::Corrupt) => self.shared.store.quarantine(capability),
                Some(CatalogAccountPreimage::Missing | CatalogAccountPreimage::Present(_)) => {
                    self.shared.store.purge(capability)
                }
                None => Err(StateStoreError::Unavailable),
            };
            if let Err(error) = result.map_err(state_error) {
                let rollback = rollback_catalog_reconciliation(
                    &self.shared,
                    &mut state,
                    &previous_state,
                    &previous,
                    &preimages,
                    completed_purges,
                    now_epoch,
                );
                return Err(preserve_catalog_error(error, rollback));
            }
        }

        let account_capabilities = state.accounts.keys().cloned().collect::<Vec<_>>();
        for capability in account_capabilities {
            let Some(entry) = state.accounts.get_mut(&capability) else {
                continue;
            };
            match next.get(&capability) {
                None => {
                    if !entry.revoked {
                        revoke_entry(entry, now_epoch, revocation_sample);
                    }
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
        if let Err(error) = persist_cooldown_tombstones(
            &self.shared,
            &state,
            &removed_capabilities,
            &preimages,
            now_epoch,
            revocation_sample,
        ) {
            let primary = state_error(error);
            let rollback = rollback_catalog_reconciliation(
                &self.shared,
                &mut state,
                &previous_state,
                &previous,
                &preimages,
                completed_purges.clone(),
                now_epoch,
            );
            return Err(preserve_catalog_error(primary, rollback));
        }
        if let Err(error) = persist_reset_envelopes(
            &self.shared,
            &state,
            &reset_capabilities,
            &preimages,
            revocation_sample,
        ) {
            let primary = state_error(error);
            let rollback = rollback_catalog_reconciliation(
                &self.shared,
                &mut state,
                &previous_state,
                &previous,
                &preimages,
                completed_purges.clone(),
                now_epoch,
            );
            return Err(preserve_catalog_error(primary, rollback));
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
