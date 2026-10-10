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
    StateStoreError, UsageCoordinator, account_cooldown_deadline, catalog_entries_from_map,
    catalog_purge_set, cooldown_tombstone, first_catalog_rollback_error, pending_attempt_envelope,
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
            let has_active_persisted_deadline = account_cooldown_deadline(&envelope)
                .is_some_and(|deadline| deadline > persist_epoch);
            if state.accounts.contains_key(capability) || has_active_persisted_deadline {
                return None;
            }
            match preimages.get(capability) {
                Some(CatalogAccountPreimage::Present(preimage)) => {
                    recovered_attempt_deadline_epoch(preimage, clock_sample)
                }
                Some(CatalogAccountPreimage::Missing | CatalogAccountPreimage::Corrupt) | None => {
                    None
                }
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

struct CatalogReconciliationPlan {
    previous_state: CoordinatorState,
    previous: BTreeMap<UsageAccountCapability, String>,
    next: BTreeMap<UsageAccountCapability, String>,
    purge: BTreeSet<UsageAccountCapability>,
    removed_capabilities: BTreeSet<UsageAccountCapability>,
    reset_capabilities: BTreeSet<UsageAccountCapability>,
    preimages: BTreeMap<UsageAccountCapability, CatalogAccountPreimage>,
    now_epoch: i64,
    clock_sample: super::ClockSample,
}

fn persist_catalog_reconciliation(
    shared: &Arc<Shared>,
    state: &mut CoordinatorState,
    plan: &mut CatalogReconciliationPlan,
) -> Result<(), UsageCoordinationError> {
    let mut completed_purges = BTreeSet::new();
    let mut prewritten_tombstones = BTreeSet::new();
    for capability in &plan.purge {
        let is_removed = plan.removed_capabilities.contains(capability);
        let Some((tombstone, persist_epoch)) = catalog_purge_tombstone(
            state,
            &plan.preimages,
            capability,
            is_removed,
            plan.now_epoch,
            plan.clock_sample,
        ) else {
            continue;
        };

        // Install every cooldown tombstone or pending marker before any
        // catalog purge. A crash or later purge failure then cannot erase
        // the only durable invocation and retry fence.
        if matches!(
            plan.preimages.get(capability),
            Some(CatalogAccountPreimage::Missing)
        ) {
            plan.preimages.insert(
                capability.clone(),
                CatalogAccountPreimage::Present(Box::new(tombstone.clone())),
            );
        }
        completed_purges.insert(capability.clone());
        if let Err(error) = shared.store.store(&tombstone, persist_epoch) {
            let primary = state_error(error);
            let rollback = rollback_catalog_reconciliation(
                shared,
                state,
                &plan.previous_state,
                &plan.previous,
                &plan.preimages,
                completed_purges.clone(),
                plan.now_epoch,
            );
            return Err(preserve_catalog_error(primary, rollback));
        }
        prewritten_tombstones.insert(capability.clone());
    }
    for capability in &plan.purge {
        if prewritten_tombstones.contains(capability) {
            continue;
        }
        // A store can report failure after the unlink/quarantine reached
        // disk. Restore this preimage on every failure path, not only
        // after a fully successful purge return.
        completed_purges.insert(capability.clone());
        let result: Result<(), StateStoreError> = match plan.preimages.get(capability) {
            Some(CatalogAccountPreimage::Corrupt) => shared.store.quarantine(capability),
            Some(CatalogAccountPreimage::Missing | CatalogAccountPreimage::Present(_)) => {
                shared.store.purge(capability)
            }
            None => Err(StateStoreError::Unavailable),
        };
        if let Err(error) = result.map_err(state_error) {
            let rollback = rollback_catalog_reconciliation(
                shared,
                state,
                &plan.previous_state,
                &plan.previous,
                &plan.preimages,
                completed_purges,
                plan.now_epoch,
            );
            return Err(preserve_catalog_error(error, rollback));
        }
    }

    let account_capabilities = state.accounts.keys().cloned().collect::<Vec<_>>();
    for capability in account_capabilities {
        let Some(entry) = state.accounts.get_mut(&capability) else {
            continue;
        };
        match plan.next.get(&capability) {
            None => {
                if !entry.revoked {
                    revoke_entry(entry, plan.now_epoch, plan.clock_sample);
                }
                state.blocked.remove(&capability);
            }
            Some(revision)
                if entry.revoked || entry.catalog_revision.as_ref() != Some(revision) =>
            {
                reset_entry(entry, plan.now_epoch, revision.clone());
                state.blocked.remove(&capability);
            }
            Some(_) => {}
        }
    }
    if let Err(error) = persist_reset_envelopes(
        shared,
        state,
        &plan.reset_capabilities,
        &plan.preimages,
        &prewritten_tombstones,
        plan.clock_sample,
    ) {
        let primary = state_error(error);
        let rollback = rollback_catalog_reconciliation(
            shared,
            state,
            &plan.previous_state,
            &plan.previous,
            &plan.preimages,
            completed_purges,
            plan.now_epoch,
        );
        return Err(preserve_catalog_error(primary, rollback));
    }
    Ok(())
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

        let mut plan = CatalogReconciliationPlan {
            previous_state,
            previous,
            next,
            purge,
            removed_capabilities,
            reset_capabilities,
            preimages,
            now_epoch,
            clock_sample: self.shared.clock.sample(now_epoch),
        };
        persist_catalog_reconciliation(&self.shared, &mut state, &mut plan)?;

        let CatalogReconciliationPlan {
            previous_state,
            previous,
            next,
            preimages,
            ..
        } = plan;
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
