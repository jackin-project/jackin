// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Catalog reconciliation.

use std::collections::{BTreeMap, BTreeSet};

use std::sync::Arc;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind,
};

use super::{
    CatalogAccountPreimage, CoordinatorState, Shared, StateStoreError, coordination_error,
};

pub(crate) fn validate_catalog_entries(
    entries: &[UsageCatalogEntry],
) -> Result<(), UsageCoordinationError> {
    let mut capabilities = BTreeSet::new();
    for entry in entries {
        if entry.revision.is_empty() || !capabilities.insert(entry.capability.clone()) {
            return Err(coordination_error(
                UsageCoordinationErrorKind::CorruptState,
                "usage broker catalog contains an invalid or duplicate entry",
            ));
        }
    }
    Ok(())
}

pub(crate) fn catalog_purge_set(
    state: &CoordinatorState,
    previous: &BTreeMap<UsageAccountCapability, String>,
    next: &BTreeMap<UsageAccountCapability, String>,
) -> BTreeSet<UsageAccountCapability> {
    let mut purge = BTreeSet::new();
    for (capability, revision) in previous {
        if next
            .get(capability)
            .is_none_or(|current| current != revision)
        {
            purge.insert(capability.clone());
        }
    }
    for (capability, entry) in &state.accounts {
        if next
            .get(capability)
            .is_none_or(|revision| entry.catalog_revision.as_ref() != Some(revision))
        {
            purge.insert(capability.clone());
        }
    }
    for capability in state.blocked.keys() {
        if !next.contains_key(capability) {
            purge.insert(capability.clone());
        }
    }
    purge
}

pub(crate) fn catalog_entries_from_map(
    catalog: &BTreeMap<UsageAccountCapability, String>,
) -> Vec<UsageCatalogEntry> {
    catalog
        .iter()
        .map(|(capability, revision)| UsageCatalogEntry {
            capability: capability.clone(),
            revision: revision.clone(),
        })
        .collect()
}

pub(crate) fn reconcile_executor_catalog(
    shared: &Arc<Shared>,
    catalog_revision: Option<&str>,
    entries: &[UsageCatalogEntry],
) -> Result<(), UsageCoordinationError> {
    match catalog_revision {
        Some(catalog_revision) => shared
            .executor
            .reconcile_catalog_revision(catalog_revision, entries),
        None => shared.executor.reconcile_catalog(entries),
    }
}

pub(crate) fn restore_catalog_preimages(
    shared: &Arc<Shared>,
    preimages: &BTreeMap<UsageAccountCapability, CatalogAccountPreimage>,
    capabilities: BTreeSet<UsageAccountCapability>,
    now_epoch: i64,
) -> Result<(), StateStoreError> {
    for capability in capabilities {
        let Some(preimage) = preimages.get(&capability) else {
            return Err(StateStoreError::Unavailable);
        };
        match preimage {
            CatalogAccountPreimage::Present(envelope) => {
                shared.store.store(envelope, now_epoch)?;
            }
            CatalogAccountPreimage::Missing => {
                shared.store.purge(&capability)?;
            }
            // The invalid bytes were deliberately quarantined during the
            // failed rotation. Reintroducing them would re-poison recovery.
            CatalogAccountPreimage::Corrupt => {}
        }
    }
    Ok(())
}

pub(crate) fn first_catalog_rollback_error(
    durable: Result<(), UsageCoordinationError>,
    executor: Result<(), UsageCoordinationError>,
) -> Result<(), UsageCoordinationError> {
    match (durable, executor) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), _) | (Ok(()), Err(error)) => Err(error),
    }
}

pub(crate) fn preserve_catalog_error(
    primary: UsageCoordinationError,
    rollback: Result<(), UsageCoordinationError>,
) -> UsageCoordinationError {
    match rollback {
        Ok(()) => primary,
        Err(rollback) => coordination_error(
            primary.kind,
            format!(
                "{}; catalog rollback failed: {}",
                primary.message, rollback.message
            ),
        ),
    }
}
