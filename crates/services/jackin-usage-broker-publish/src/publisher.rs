// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Projection publisher.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageIdentityKindV1,
    UsageProjectionRefreshStateV1, UsageProjectionV1, UsageRefreshPhase,
};

use jackin_usage_coordinator::{
    FileProjectionStateStore, ProjectionStateEnvelope, UsageCoordinator,
};

/// Server-side incremental publisher. Cheap to clone; all state is shared.
use super::{
    CatalogDiagnostics, apply_catalog_diagnostics, catalog_entries, catalog_revision_conflict,
    first_publisher_rollback_error, merge_views, preserve_publisher_error, projection_store_error,
    publisher_corrupt_state, publisher_unavailable, retain_revoked_accounts,
};

/// Server-side incremental publisher. Cheap to clone; all state is shared.
#[derive(Debug, Clone)]
pub struct ProjectionPublisher {
    coordinator: Arc<UsageCoordinator>,
    projection: Arc<Mutex<UsageProjectionV1>>,
    store: FileProjectionStateStore,
    known: Arc<Mutex<BTreeSet<UsageAccountCapability>>>,
    published: Arc<Mutex<BTreeMap<UsageAccountCapability, PublishedAccount>>>,
    catalog: Arc<Mutex<Option<BTreeMap<UsageAccountCapability, String>>>>,
    /// Serializes catalog replacement with incremental publication and
    /// observed-capability admission.
    catalog_lifecycle: Arc<Mutex<()>>,
    identity_metadata: BTreeMap<UsageAccountCapability, AccountIdentityMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PublishedAccount {
    generation: u64,
    phase: UsageRefreshPhase,
    has_snapshot: bool,
}

/// Canonical identity evidence captured by host discovery and carried into
/// the Capsule-facing projection. A display label is not identity evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountIdentityMetadata {
    pub identity_kind: UsageIdentityKindV1,
    pub provenance_count: u32,
}

impl ProjectionPublisher {
    /// Attach a publisher to one broker-owned coordinator and projection.
    pub fn new(
        coordinator: Arc<UsageCoordinator>,
        projection: Arc<Mutex<UsageProjectionV1>>,
        store: FileProjectionStateStore,
    ) -> Self {
        Self {
            coordinator,
            projection,
            store,
            known: Arc::new(Mutex::new(BTreeSet::new())),
            published: Arc::new(Mutex::new(BTreeMap::new())),
            catalog: Arc::new(Mutex::new(None)),
            catalog_lifecycle: Arc::new(Mutex::new(())),
            identity_metadata: BTreeMap::new(),
        }
    }

    /// Attach the last durable catalog. Existing materialized projection rows
    /// are observed; newly admitted catalog members are not, so discovery
    /// cannot expand the manifest/tab surface by itself.
    #[must_use]
    pub fn with_catalog(self, entries: impl IntoIterator<Item = UsageCatalogEntry>) -> Self {
        let catalog = entries
            .into_iter()
            .map(|entry| (entry.capability, entry.revision))
            .collect::<BTreeMap<_, _>>();
        if let (Ok(projection), Ok(mut known)) = (self.projection.lock(), self.known.lock()) {
            known.extend(
                projection
                    .providers
                    .iter()
                    .flat_map(|provider| {
                        provider
                            .accounts
                            .iter()
                            .map(|account| UsageAccountCapability {
                                account_id: account.canonical_account_id.clone(),
                                surface_id: provider.provider_id.clone(),
                            })
                    })
                    .filter(|capability| catalog.contains_key(capability)),
            );
        }
        if let Ok(mut current) = self.catalog.lock() {
            *current = Some(catalog);
        }
        self
    }

    /// Attach the immutable host-discovery identity evidence used by the
    /// Capsule/FFI publication. Missing entries remain conservative fallback
    /// rows for synthetic broker seams only.
    #[must_use]
    pub fn with_identity_metadata(
        mut self,
        identity_metadata: BTreeMap<UsageAccountCapability, AccountIdentityMetadata>,
    ) -> Self {
        self.identity_metadata = identity_metadata;
        self
    }

    /// Record one capability served by the broker. Only observed capabilities
    /// are ever merged into a publication.
    pub fn observe(&self, capability: &UsageAccountCapability) {
        let Ok(_catalog_lifecycle) = self.catalog_lifecycle.lock() else {
            return;
        };
        let admitted = self.catalog.lock().is_ok_and(|catalog| {
            catalog
                .as_ref()
                .is_none_or(|catalog| catalog.contains_key(capability))
        });
        if admitted && let Ok(mut known) = self.known.lock() {
            known.insert(capability.clone());
        }
    }

    /// Capabilities observed so far, in settled order.
    pub fn known_capabilities(&self) -> Vec<UsageAccountCapability> {
        let Ok(_catalog_lifecycle) = self.catalog_lifecycle.lock() else {
            return Vec::new();
        };
        self.known_capabilities_locked()
    }

    pub fn known_capabilities_locked(&self) -> Vec<UsageAccountCapability> {
        let Ok(catalog) = self.catalog.lock() else {
            return Vec::new();
        };
        let known = self
            .known
            .lock()
            .map(|known| known.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        catalog.as_ref().map_or(known.clone(), |catalog| {
            known
                .into_iter()
                .filter(|capability| catalog.contains_key(capability))
                .collect()
        })
    }

    /// Replace the broker catalog and publish the revocation transaction.
    pub fn reconcile_catalog(
        &self,
        catalog_revision: String,
        entries: Vec<UsageCatalogEntry>,
        now_epoch: i64,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        self.reconcile_catalog_if_projection(None, catalog_revision, entries, now_epoch)
    }

    /// Replace the catalog only when the caller still owns the observed
    /// publication lease. Every shared lock is acquired before durable or
    /// coordinator mutation; the old envelope is restored if coordinator
    /// reconciliation fails.
    pub fn reconcile_catalog_if_projection(
        &self,
        expected_projection_id: Option<&str>,
        catalog_revision: String,
        entries: Vec<UsageCatalogEntry>,
        now_epoch: i64,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        self.reconcile_catalog_inner(
            expected_projection_id,
            catalog_revision,
            entries,
            None,
            now_epoch,
        )
    }

    /// Replace the catalog and its scan diagnostics in one durable transaction.
    /// Only catalog-derived issue codes are replaced; other issue records and
    /// canonical account state remain intact.
    pub fn reconcile_catalog_if_projection_with_diagnostics(
        &self,
        expected_projection_id: Option<&str>,
        catalog_revision: String,
        entries: Vec<UsageCatalogEntry>,
        diagnostics: CatalogDiagnostics,
        now_epoch: i64,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        self.reconcile_catalog_inner(
            expected_projection_id,
            catalog_revision,
            entries,
            Some(diagnostics),
            now_epoch,
        )
    }

    fn reconcile_catalog_inner(
        &self,
        expected_projection_id: Option<&str>,
        catalog_revision: String,
        entries: Vec<UsageCatalogEntry>,
        diagnostics: Option<CatalogDiagnostics>,
        now_epoch: i64,
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
        let _catalog_lifecycle = self
            .catalog_lifecycle
            .lock()
            .map_err(|_| publisher_unavailable())?;
        let mut current_catalog = self.catalog.lock().map_err(|_| publisher_unavailable())?;
        let mut known = self.known.lock().map_err(|_| publisher_unavailable())?;
        let mut published = self.published.lock().map_err(|_| publisher_unavailable())?;
        let mut projection = self
            .projection
            .lock()
            .map_err(|_| publisher_unavailable())?;
        if expected_projection_id.is_some_and(|expected| expected != projection.projection_id) {
            return Err(catalog_revision_conflict());
        }
        let mut catalog = BTreeMap::new();
        for entry in &entries {
            if entry.revision.is_empty()
                || catalog
                    .insert(entry.capability.clone(), entry.revision.clone())
                    .is_some()
            {
                return Err(publisher_corrupt_state());
            }
        }
        let previous = projection.clone();
        let mut next = projection.clone();
        retain_revoked_accounts(&mut next, &previous, &catalog, current_catalog.as_ref());
        if let Some(diagnostics) = diagnostics.as_ref() {
            apply_catalog_diagnostics(&mut next, diagnostics);
        }
        next.discovery_revision = catalog_revision.clone();
        next.broker_generation = next.broker_generation.saturating_add(1);
        next.projection_id = format!("{}:{}", next.broker_instance_id, next.broker_generation);
        next.generated_at_epoch = now_epoch;
        next.refresh_state = UsageProjectionRefreshStateV1::Idle;
        if next.validate().is_err() {
            return Err(publisher_corrupt_state());
        }
        let previous_envelope = self.store.load().map_err(projection_store_error)?;
        let envelope = ProjectionStateEnvelope {
            schema_version: ProjectionStateEnvelope::SCHEMA_VERSION,
            catalog_revision: next.discovery_revision.clone(),
            catalog: catalog_entries(&catalog),
            broker_instance_id: next.broker_instance_id.clone(),
            projection: next.clone(),
            aliases: previous_envelope
                .as_ref()
                .map_or_else(Vec::new, |envelope| envelope.aliases.clone()),
            retry_deadline_epoch: None,
            success_deadline_epoch: None,
        };
        let transaction = self
            .coordinator
            .reconcile_catalog_transaction_with_revision(
                Some(catalog_revision.as_str()),
                entries.iter().cloned(),
                now_epoch,
            )?;
        if let Err(error) = self.store.store(&envelope) {
            let primary = projection_store_error(error);
            let projection_restore = match previous_envelope.as_ref() {
                Some(previous) => self.store.store(previous),
                None => self.store.clear(),
            };
            let coordinator_restore = transaction.rollback(now_epoch);
            return Err(preserve_publisher_error(
                primary,
                first_publisher_rollback_error(projection_restore, coordinator_restore),
            ));
        }

        *current_catalog = Some(catalog.clone());
        known.retain(|capability| catalog.contains_key(capability));
        published.retain(|capability, _| catalog.contains_key(capability));
        *projection = next.clone();
        drop(transaction);
        Ok(next)
    }

    /// Read one publication under the same boundary used for catalog
    /// replacement and observed-capability admission.
    pub fn current_projection(&self) -> Result<UsageProjectionV1, UsageCoordinationError> {
        let _catalog_lifecycle = self
            .catalog_lifecycle
            .lock()
            .map_err(|_| publisher_unavailable())?;
        self.projection
            .lock()
            .map(|projection| projection.clone())
            .map_err(|_| publisher_unavailable())
    }

    /// Merge every observed account's latest state and publish when anything
    /// advanced. Returns whether a new publication was stored.
    ///
    /// Each account is read independently: one unreadable account is skipped
    /// without affecting the others.
    pub fn publish_due(&self, now_epoch: i64) -> bool {
        let Ok(_catalog_lifecycle) = self.catalog_lifecycle.lock() else {
            return false;
        };
        let capabilities = self.known_capabilities_locked();
        if capabilities.is_empty() {
            return false;
        }
        let mut views = Vec::with_capacity(capabilities.len());
        for capability in &capabilities {
            if let Ok(view) = self.coordinator.current(capability, now_epoch) {
                views.push(view);
            }
        }
        if views.is_empty() {
            return false;
        }
        let Ok(mut published) = self.published.lock() else {
            return false;
        };
        let mut pending = Vec::new();
        let mut advanced = false;
        for view in &views {
            let current = PublishedAccount {
                generation: view.generation,
                phase: view.phase,
                has_snapshot: view.snapshot.is_some(),
            };
            // Older generations and resurrected activity never regress a
            // publication; unchanged accounts publish nothing new.
            let stale = published.get(&view.capability).is_some_and(|previous| {
                *previous == current
                    || previous.generation > current.generation
                    || (previous.generation == current.generation
                        && previous.phase.is_terminal()
                        && current.phase.is_active())
            });
            if !stale {
                pending.push((view.capability.clone(), current));
                advanced = true;
            }
        }
        if !advanced {
            return false;
        }
        let Ok(mut projection) = self.projection.lock() else {
            return false;
        };
        let previous = projection.clone();
        let mut next = projection.clone();
        merge_views(&mut next, &views, &self.identity_metadata);
        if let Ok(catalog) = self.catalog.lock()
            && let Some(catalog) = catalog.as_ref()
        {
            retain_revoked_accounts(&mut next, &previous, catalog, None);
        }
        next.broker_generation = next.broker_generation.saturating_add(1);
        next.projection_id = format!("{}:{}", next.broker_instance_id, next.broker_generation);
        next.generated_at_epoch = now_epoch;
        if next.validate().is_err() {
            return false;
        }
        let envelope = ProjectionStateEnvelope {
            schema_version: ProjectionStateEnvelope::SCHEMA_VERSION,
            catalog_revision: next.discovery_revision.clone(),
            catalog: self
                .catalog
                .lock()
                .ok()
                .and_then(|catalog| catalog.as_ref().map(catalog_entries))
                .unwrap_or_default(),
            broker_instance_id: next.broker_instance_id.clone(),
            projection: next.clone(),
            aliases: Vec::new(),
            retry_deadline_epoch: None,
            success_deadline_epoch: None,
        };
        if self.store.store(&envelope).is_err() {
            return false;
        }
        *projection = next;
        for (capability, current) in pending {
            published.insert(capability, current);
        }
        true
    }
}
