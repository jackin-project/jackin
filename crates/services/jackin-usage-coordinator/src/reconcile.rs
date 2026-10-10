// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Catalog reconciliation methods.

use std::collections::{BTreeMap, BTreeSet};

use std::sync::Arc;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageRefreshPhase,
};

use super::entries::{monotonic_cooldown_deadline_epoch, recovered_attempt_deadline_epoch};
use super::{
    AccountStateEnvelope, CatalogAccountPreimage, CatalogTransaction, CoordinatorState, Shared,
    StateStoreError, UsageCoordinator, catalog_entries_from_map, catalog_purge_set,
    cooldown_tombstone, first_catalog_rollback_error, pending_attempt_envelope,
    preserve_catalog_error, reconcile_executor_catalog, reset_entry, reset_envelope,
    restore_catalog_preimages, revoke_entry, revoke_envelope, state_error, unavailable_error,
    validate_catalog_entries,
};

fn persist_reset_envelopes(
    shared: &Shared,
    state: &CoordinatorState,
    reset_capabilities: &BTreeSet<UsageAccountCapability>,
    preimages: &BTreeMap<UsageAccountCapability, CatalogAccountPreimage>,
    prewritten_tombstones: &BTreeSet<UsageAccountCapability>,
    mutated_capabilities: &mut BTreeSet<UsageAccountCapability>,
    clock_sample: super::ClockSample,
) -> Result<(), StateStoreError> {
    for capability in reset_capabilities {
        if prewritten_tombstones.contains(capability) {
            continue;
        }
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
            mutated_capabilities.insert(capability.clone());
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

fn catalog_purge_tombstone(
    state: &CoordinatorState,
    preimages: &BTreeMap<UsageAccountCapability, CatalogAccountPreimage>,
    capability: &UsageAccountCapability,
    is_removed: bool,
    now_epoch: i64,
    clock_sample: super::ClockSample,
) -> Option<(AccountStateEnvelope, i64)> {
    let already_revoked = state
        .accounts
        .get(capability)
        .is_some_and(|entry| entry.revoked);
    let pending_attempt = has_pending_provider_attempt(state, preimages, capability);
    let mut envelope = state
        .accounts
        .get(capability)
        .map(|entry| entry.envelope.clone())
        .or_else(|| match preimages.get(capability) {
            Some(CatalogAccountPreimage::Present(envelope)) => Some((**envelope).clone()),
            Some(CatalogAccountPreimage::Corrupt) if capability.surface_id == "claude" => {
                // Corrupt durable state cannot establish whether a Claude
                // provider attempt already happened. Replace it with a
                // result-free uncertainty marker before catalog cleanup.
                let mut marker = AccountStateEnvelope::idle(capability.clone());
                marker.reload_fence_required = true;
                Some(marker)
            }
            Some(CatalogAccountPreimage::Missing | CatalogAccountPreimage::Corrupt) | None => None,
        })?;
    if is_removed {
        if !already_revoked {
            revoke_envelope(&mut envelope, now_epoch, clock_sample);
        }
    } else {
        reset_envelope(&mut envelope);
    }

    let persist_epoch = clock_sample
        .ceil_epoch()
        .max(envelope.provider_invoked_at_epoch.unwrap_or(0));
    let runtime_deadline = state
        .accounts
        .get(capability)
        .and_then(|entry| monotonic_cooldown_deadline_epoch(entry, clock_sample))
        .or_else(|| {
            if state.accounts.contains_key(capability) {
                None
            } else {
                recovered_attempt_deadline_epoch(&envelope, clock_sample)
            }
        });
    if let Some(runtime_deadline) = runtime_deadline {
        envelope.retry_deadline_epoch = Some(
            envelope
                .retry_deadline_epoch
                .map_or(runtime_deadline, |deadline| deadline.max(runtime_deadline)),
        );
    }

    let tombstone = if is_removed {
        cooldown_tombstone(&envelope, persist_epoch, pending_attempt)
    } else if pending_attempt {
        Some(pending_attempt_envelope(&envelope, persist_epoch))
    } else {
        cooldown_tombstone(&envelope, persist_epoch, false)
    }?;
    Some((tombstone, persist_epoch))
}

fn rollback_catalog_reconciliation(
    shared: &Arc<Shared>,
    state: &mut CoordinatorState,
    previous_state: &CoordinatorState,
    previous: &BTreeMap<UsageAccountCapability, String>,
    preimages: &BTreeMap<UsageAccountCapability, CatalogAccountPreimage>,
    mutated_capabilities: BTreeSet<UsageAccountCapability>,
    now_epoch: i64,
) -> Result<(), UsageCoordinationError> {
    let durable = restore_catalog_preimages(shared, preimages, mutated_capabilities, now_epoch)
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
    /// Removed capabilities are fenced in memory and their result-free
    /// cooldown or uncertainty markers are durably written before cleanup.
    /// Revision changes fence old work and discard its materialized result
    /// before the capability can refresh again.
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
        let mut mutated_capabilities = BTreeSet::new();
        let mut prewritten_tombstones = BTreeSet::new();
        for capability in &purge {
            let Some((tombstone, persist_epoch)) = catalog_purge_tombstone(
                &state,
                &preimages,
                capability,
                removed_capabilities.contains(capability),
                now_epoch,
                revocation_sample,
            ) else {
                continue;
            };
            if matches!(
                preimages.get(capability),
                Some(CatalogAccountPreimage::Missing | CatalogAccountPreimage::Corrupt)
            ) {
                // If a later write fails, rollback must keep the newly
                // materialized safety marker rather than restore absence or
                // unreadable bytes and erase an invocation uncertainty fence.
                preimages.insert(
                    capability.clone(),
                    CatalogAccountPreimage::Present(Box::new(tombstone.clone())),
                );
            }
            // The store can report an error after an atomic replacement has
            // reached disk. Include this path in rollback before calling it.
            mutated_capabilities.insert(capability.clone());
            if let Err(error) = self.shared.store.store(&tombstone, persist_epoch) {
                let primary = state_error(error);
                let rollback = rollback_catalog_reconciliation(
                    &self.shared,
                    &mut state,
                    &previous_state,
                    &previous,
                    &preimages,
                    mutated_capabilities.clone(),
                    now_epoch,
                );
                return Err(preserve_catalog_error(primary, rollback));
            }
            prewritten_tombstones.insert(capability.clone());
        }
        for capability in &purge {
            if prewritten_tombstones.contains(capability) {
                continue;
            }
            // A purge/quarantine implementation may report failure after its
            // filesystem mutation. Restore this preimage on every failure.
            mutated_capabilities.insert(capability.clone());
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
                    mutated_capabilities,
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
                    reset_entry(entry, now_epoch, revocation_sample, revision.clone());
                    state.blocked.remove(&capability);
                }
                Some(_) => {}
            }
        }
        if let Err(error) = persist_reset_envelopes(
            &self.shared,
            &state,
            &reset_capabilities,
            &preimages,
            &prewritten_tombstones,
            &mut mutated_capabilities,
            revocation_sample,
        ) {
            let primary = state_error(error);
            let rollback = rollback_catalog_reconciliation(
                &self.shared,
                &mut state,
                &previous_state,
                &previous,
                &preimages,
                mutated_capabilities,
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
