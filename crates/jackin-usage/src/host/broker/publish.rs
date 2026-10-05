// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Incremental per-account publication of the canonical broker projection.
//!
//! The broker publishes one immutable [`UsageProjectionV2`] per change. Each
//! account merges independently as its generation completes: one stalled
//! account never blocks healthy accounts, and a stalled account keeps its
//! loading/refreshing state instead of receiving fabricated data.
//!
//! Publication rules:
//!
//! - The catalog revision (`discovery_revision`) is fixed for the broker
//!   process lifetime; every publication carries the same revision while
//!   `broker_generation` increases monotonically.
//! - Only capabilities observed on broker traffic are merged. Unknown or
//!   never-requested accounts are never fabricated, and per-account published
//!   generations only move forward, so older generations can never regress a
//!   publication.
//! - A publication that fails [`UsageProjectionV2::validate`] is discarded and
//!   the last-good publication is kept.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use jackin_protocol::control::{FocusedUsageView, UsageSnapshotStatus};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageAccountV2, UsageCatalogEntry, UsageCoordinationError,
    UsageCoordinationErrorKind, UsageFreshnessPhaseV2, UsageFreshnessV2, UsageGenerationView,
    UsageIdentityKindV2, UsageIssueRecoverabilityV2, UsageIssueScopeV2, UsageIssueV2,
    UsageLifecycleV2, UsageLimitWindowV2, UsageMembershipStateV2, UsageProjectionRefreshStateV2,
    UsageProjectionV2, UsageProviderV2, UsageRefreshPhase, UsageUnresolvedV2,
};

use crate::coordinator::{
    FileProjectionStateStore, ProjectionStateEnvelope, StateStoreError, UsageCoordinator,
    PROJECTION_STATE_SCHEMA_VERSION,
};

use super::super::HostSurfaceId;
use super::super::accounts::{CanonicalAccountIdentity, CanonicalAccountSubject};
use super::super::projection::{
    failure_lifecycle, lifecycle, metric_groups_for_view, project_window,
};

type AcceptedCatalog = BTreeMap<UsageAccountCapability, UsageCatalogEntry>;

/// Server-side incremental publisher. Cheap to clone; all state is shared.
#[derive(Debug, Clone)]
pub(crate) struct ProjectionPublisher {
    coordinator: Arc<UsageCoordinator>,
    projection: Arc<Mutex<UsageProjectionV2>>,
    store: FileProjectionStateStore,
    known: Arc<Mutex<BTreeSet<UsageAccountCapability>>>,
    published: Arc<Mutex<BTreeMap<UsageAccountCapability, PublishedAccount>>>,
    catalog: Arc<Mutex<Option<AcceptedCatalog>>>,
    /// Serializes catalog replacement with incremental publication and
    /// observed-capability admission.
    catalog_lifecycle: Arc<Mutex<()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PublishedAccount {
    generation: u64,
    phase: UsageRefreshPhase,
    has_snapshot: bool,
}

impl ProjectionPublisher {
    /// Attach a publisher to one broker-owned coordinator and projection.
    pub(crate) fn new(
        coordinator: Arc<UsageCoordinator>,
        projection: Arc<Mutex<UsageProjectionV2>>,
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
        }
    }

    /// Attach the complete accepted durable catalog. Restore observation
    /// tracking only for existing materialized generations; inventory admission
    /// itself does not request a provider refresh.
    pub(crate) fn with_catalog(
        self,
        entries: impl IntoIterator<Item = UsageCatalogEntry>,
    ) -> Result<Self, UsageCoordinationError> {
        let catalog = accepted_catalog(entries)?;
        {
            let projection = self
                .projection
                .lock()
                .map_err(|_| publisher_unavailable())?;
            projection
                .validate()
                .map_err(|_| publisher_corrupt_state())?;
            for source in &projection.unresolved {
                if !catalog.values().any(|entry| {
                    entry.canonical_identity.is_none()
                        && entry.capability.account_id == source.capability_id
                        && HostSurfaceId::from_id(&entry.capability.surface_id)
                            .is_some_and(|surface| surface.provider_id() == source.provider_id)
                        && entry.provenance_count == source.configuration_count
                }) {
                    return Err(publisher_corrupt_state());
                }
            }
            for provider in &projection.providers {
                for account in &provider.accounts {
                    if account.refresh_capabilities.is_empty()
                        && !(account.status_label.as_deref() == Some("removed")
                            && account.lifecycle == UsageLifecycleV2::Unavailable
                            && account.freshness.phase == UsageFreshnessPhaseV2::Failed
                            && account.windows.is_empty()
                            && account.metric_groups.is_empty()
                            && account.issues.is_empty())
                    {
                        return Err(publisher_corrupt_state());
                    }
                    for route in &account.refresh_capabilities {
                        let entry = catalog.get(route).ok_or_else(publisher_corrupt_state)?;
                        let identity = catalog_identity(entry)
                            .map_err(|_| publisher_corrupt_state())?
                            .ok_or_else(publisher_corrupt_state)?;
                        if identity.canonical_id_v1() != account.canonical_account_id
                            || identity.surface.provider_id() != provider.provider_id
                            || canonical_identity_kind(&identity) != account.identity_kind
                            || entry.provenance_count != account.provenance_count
                        {
                            return Err(publisher_corrupt_state());
                        }
                    }
                }
            }
        }
        {
            let projection = self
                .projection
                .lock()
                .map_err(|_| publisher_unavailable())?;
            let mut known = self.known.lock().map_err(|_| publisher_unavailable())?;
            known.extend(
                projection
                    .providers
                    .iter()
                    .flat_map(|provider| provider.accounts.iter())
                    .filter(|account| account.freshness.generation != 0)
                    .flat_map(|account| account.refresh_capabilities.iter().cloned())
                    .filter(|capability| catalog.contains_key(capability)),
            );
        }
        *self.catalog.lock().map_err(|_| publisher_unavailable())? = Some(catalog);
        Ok(self)
    }

    /// Record one capability served by the broker. Only observed capabilities
    /// are ever merged into a publication.
    pub(crate) fn observe(&self, capability: &UsageAccountCapability) {
        let Ok(_catalog_lifecycle) = self.catalog_lifecycle.lock() else {
            return;
        };
        let admitted = self.catalog.lock().is_ok_and(|catalog| {
            catalog
                .as_ref()
                .is_some_and(|catalog| catalog.contains_key(capability))
        });
        if admitted && let Ok(mut known) = self.known.lock() {
            known.insert(capability.clone());
        }
    }

    /// Capabilities observed so far, in settled order.
    pub(crate) fn known_capabilities(&self) -> Vec<UsageAccountCapability> {
        let Ok(_catalog_lifecycle) = self.catalog_lifecycle.lock() else {
            return Vec::new();
        };
        self.known_capabilities_locked()
    }

    fn known_capabilities_locked(&self) -> Vec<UsageAccountCapability> {
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
    pub(crate) fn reconcile_catalog(
        &self,
        catalog_revision: String,
        entries: Vec<UsageCatalogEntry>,
        now_epoch: i64,
    ) -> Result<UsageProjectionV2, UsageCoordinationError> {
        self.reconcile_catalog_if_projection(None, catalog_revision, entries, now_epoch)
    }

    /// Replace the catalog only when the caller still owns the observed
    /// publication lease. Every shared lock is acquired before durable or
    /// coordinator mutation; the old envelope is restored if coordinator
    /// reconciliation fails.
    pub(crate) fn reconcile_catalog_if_projection(
        &self,
        expected_projection_id: Option<&str>,
        catalog_revision: String,
        entries: Vec<UsageCatalogEntry>,
        now_epoch: i64,
    ) -> Result<UsageProjectionV2, UsageCoordinationError> {
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
        let next_generation = next_publication_generation(projection.broker_generation)?;
        let catalog = accepted_catalog(entries.iter().cloned())?;
        let previous = projection.clone();
        let mut next = projection.clone();
        retain_revoked_accounts(&mut next, &previous, &catalog, current_catalog.as_ref())
            .map_err(|_| publisher_corrupt_state())?;
        next.discovery_revision = catalog_revision.clone();
        next.broker_generation = next_generation;
        next.projection_id = format!("{}:{}", next.broker_instance_id, next.broker_generation);
        next.generated_at_epoch = now_epoch;
        next.refresh_state = UsageProjectionRefreshStateV2::Idle;
        if next.validate().is_err() {
            return Err(publisher_corrupt_state());
        }
        let previous_envelope = self.store.load().map_err(projection_store_error)?;
        let envelope = ProjectionStateEnvelope {
            schema_version: PROJECTION_STATE_SCHEMA_VERSION,
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

        known.retain(|capability| catalog.contains_key(capability));
        published.retain(|capability, _| {
            current_catalog
                .as_ref()
                .and_then(|previous| previous.get(capability))
                == catalog.get(capability)
        });
        *current_catalog = Some(catalog.clone());
        *projection = next.clone();
        drop(transaction);
        Ok(next)
    }

    /// Read one publication under the same boundary used for catalog
    /// replacement and observed-capability admission.
    pub(crate) fn current_projection(&self) -> Result<UsageProjectionV2, UsageCoordinationError> {
        let _catalog_lifecycle = self
            .catalog_lifecycle
            .lock()
            .map_err(|_| publisher_unavailable())?;
        let projection = self
            .projection
            .lock()
            .map_err(|_| publisher_unavailable())?;
        next_publication_generation(projection.broker_generation)?;
        Ok(projection.clone())
    }

    /// Fence provider dispatch with the same boundary as catalog/publication
    /// mutation. An exhausted issuer cannot start work it cannot publish.
    pub(crate) fn admit_refresh<T>(
        &self,
        refresh: impl FnOnce() -> Result<T, UsageCoordinationError>,
    ) -> Result<T, UsageCoordinationError> {
        let _catalog_lifecycle = self
            .catalog_lifecycle
            .lock()
            .map_err(|_| publisher_unavailable())?;
        let generation = self
            .projection
            .lock()
            .map_err(|_| publisher_unavailable())?
            .broker_generation;
        next_publication_generation(generation)?;
        refresh()
    }

    /// Merge every observed account's latest state and publish when anything
    /// advanced. Returns whether a new publication was stored.
    ///
    /// Each account is read independently: one unreadable account is skipped
    /// without affecting the others.
    pub(crate) fn publish_due(&self, now_epoch: i64) -> bool {
        let Ok(_catalog_lifecycle) = self.catalog_lifecycle.lock() else {
            return false;
        };
        {
            let Ok(projection) = self.projection.lock() else {
                return false;
            };
            if next_publication_generation(projection.broker_generation).is_err() {
                return false;
            }
        }
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
        let Ok(next_generation) = next_publication_generation(projection.broker_generation) else {
            return false;
        };
        let Ok(catalog) = self.catalog.lock() else {
            return false;
        };
        let Some(catalog) = catalog.as_ref() else {
            return false;
        };
        let previous = projection.clone();
        let mut next = projection.clone();
        if merge_views(&mut next, &views, catalog).is_err() {
            return false;
        }
        if retain_revoked_accounts(&mut next, &previous, catalog, None).is_err() {
            return false;
        }
        next.broker_generation = next_generation;
        next.projection_id = format!("{}:{}", next.broker_instance_id, next.broker_generation);
        next.generated_at_epoch = now_epoch;
        if next.validate().is_err() {
            return false;
        }
        let envelope = ProjectionStateEnvelope {
            schema_version: PROJECTION_STATE_SCHEMA_VERSION,
            catalog_revision: next.discovery_revision.clone(),
            catalog: catalog_entries(catalog),
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

pub(in crate::host) fn next_publication_generation(
    current: u64,
) -> Result<u64, UsageCoordinationError> {
    current
        .checked_add(1)
        .filter(|next| *next < u64::MAX)
        .ok_or_else(|| UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message: "usage publication generation exhausted".to_owned(),
        })
}

fn accepted_catalog(
    entries: impl IntoIterator<Item = UsageCatalogEntry>,
) -> Result<AcceptedCatalog, UsageCoordinationError> {
    let mut catalog = AcceptedCatalog::new();
    let mut counts = BTreeMap::new();
    for entry in entries {
        if entry.revision.is_empty()
            || entry.capability.account_id.trim().is_empty()
            || HostSurfaceId::from_id(&entry.capability.surface_id).is_none()
        {
            return Err(publisher_corrupt_state());
        }
        if let Some(identity) = catalog_identity(&entry).map_err(|_| publisher_corrupt_state())? {
            if entry.provenance_count == 0
                || counts
                    .insert(identity.canonical_id_v1(), entry.provenance_count)
                    .is_some_and(|previous| previous != entry.provenance_count)
            {
                return Err(publisher_corrupt_state());
            }
        }
        if catalog.insert(entry.capability.clone(), entry).is_some() {
            return Err(publisher_corrupt_state());
        }
    }
    Ok(catalog)
}

fn catalog_identity(entry: &UsageCatalogEntry) -> Result<Option<CanonicalAccountIdentity>, String> {
    let Some(evidence) = &entry.canonical_identity else {
        return Ok(None);
    };
    let surface = HostSurfaceId::from_id(&entry.capability.surface_id)
        .ok_or_else(|| "catalog capability surface unavailable".to_owned())?;
    CanonicalAccountIdentity::from_protocol(surface, evidence)
        .map(Some)
        .ok_or_else(|| "catalog canonical identity does not match its capability".to_owned())
}

fn account_routes(
    catalog: &AcceptedCatalog,
) -> Result<BTreeMap<String, Vec<UsageAccountCapability>>, String> {
    let mut routes = BTreeMap::<String, Vec<_>>::new();
    for entry in catalog.values() {
        if let Some(identity) = catalog_identity(entry)? {
            routes
                .entry(identity.canonical_id_v1())
                .or_default()
                .push(entry.capability.clone());
        }
    }
    Ok(routes)
}

fn canonical_identity_kind(identity: &CanonicalAccountIdentity) -> UsageIdentityKindV2 {
    match identity.subject {
        CanonicalAccountSubject::ProviderId(_) => UsageIdentityKindV2::ProviderAccountId,
        CanonicalAccountSubject::ProviderStableHandle(_) => {
            UsageIdentityKindV2::ProviderStableHandle
        }
        CanonicalAccountSubject::SourceCapability(_) => UsageIdentityKindV2::SourceCapability,
    }
}

fn retain_revoked_accounts(
    projection: &mut UsageProjectionV2,
    previous: &UsageProjectionV2,
    catalog: &AcceptedCatalog,
    previous_catalog: Option<&AcceptedCatalog>,
) -> Result<(), String> {
    let routes = account_routes(catalog)?;
    projection.unresolved.retain(|source| {
        catalog.values().any(|entry| {
            entry.canonical_identity.is_none()
                && entry.capability.account_id == source.capability_id
                && HostSurfaceId::from_id(&entry.capability.surface_id)
                    .is_some_and(|surface| surface.provider_id() == source.provider_id)
        })
    });
    for source in &mut projection.unresolved {
        if let Some(entry) = catalog.values().find(|entry| {
            entry.canonical_identity.is_none()
                && entry.capability.account_id == source.capability_id
                && HostSurfaceId::from_id(&entry.capability.surface_id)
                    .is_some_and(|surface| surface.provider_id() == source.provider_id)
        }) {
            source.configuration_count = entry.provenance_count;
        }
    }
    for entry in catalog
        .values()
        .filter(|entry| entry.canonical_identity.is_none())
    {
        let surface = HostSurfaceId::from_id(&entry.capability.surface_id)
            .ok_or_else(|| "unknown unresolved source surface".to_owned())?;
        if !projection.unresolved.iter().any(|source| {
            source.capability_id == entry.capability.account_id
                && source.provider_id == surface.provider_id()
        }) {
            projection.unresolved.push(UsageUnresolvedV2 {
                provider_id: surface.provider_id().to_owned(),
                capability_id: entry.capability.account_id.clone(),
                configuration_count: entry.provenance_count,
                state: UsageLifecycleV2::NeedsLogin,
                issues: Vec::new(),
            });
        }
    }
    for provider in &mut projection.providers {
        for account in &mut provider.accounts {
            let current_routes = routes.get(&account.canonical_account_id);
            let current_entry = current_routes
                .and_then(|routes| routes.first())
                .and_then(|route| catalog.get(route));
            let source_identity =
                current_entry
                    .and_then(|entry| entry.canonical_identity.as_ref())
                    .is_some_and(|identity| {
                        matches!(identity.subject,
                    jackin_protocol::control::UsageCanonicalAccountSubject::SourceCapability(_))
                    });
            let changed = previous_catalog.is_some_and(|previous| {
                account.refresh_capabilities.iter().any(|route| {
                    previous
                        .get(route)
                        .zip(catalog.get(route))
                        .is_none_or(|(old, new)| {
                            old.revision != new.revision
                                || old.canonical_identity != new.canonical_identity
                        })
                })
            });
            let source_authority_retained = previous_catalog.is_some_and(|previous| {
                account.refresh_capabilities.iter().all(|route| {
                    previous.get(route).is_some_and(|old| {
                        catalog.values().any(|new| {
                            old.canonical_identity == new.canonical_identity
                                && old.revision == new.revision
                        })
                    })
                })
            });
            if current_routes.is_none() {
                mark_revoked(account);
            } else if account.status_label.as_deref() == Some("removed")
                || changed && source_identity && !source_authority_retained
            {
                account.status_label = Some("Not refreshed".to_owned());
                account.lifecycle = UsageLifecycleV2::AgentUninitialized;
                account.windows.clear();
                account.metric_groups.clear();
                account.issues.clear();
                account.auth_origin = None;
                account.freshness = UsageFreshnessV2 {
                    generation: 0,
                    phase: UsageFreshnessPhaseV2::Failed,
                    last_good_at_epoch: None,
                    retry_at_epoch: None,
                    is_stale: false,
                };
            } else if changed && account.freshness.last_good_at_epoch.is_some() {
                mark_cached_stale(account);
            }
            account.refresh_capabilities = current_routes.cloned().unwrap_or_default();
            if let Some(entry) = current_entry {
                account.provenance_count = entry.provenance_count;
                account.identity_kind = canonical_identity_kind(
                    &catalog_identity(entry)?.ok_or("accepted logical account lost identity")?,
                );
            }
        }
    }
    // An accepted logical account is inventory even before quota collection.
    for entry in catalog.values() {
        let Some(identity) = catalog_identity(entry)? else {
            continue;
        };
        let canonical_id = identity.canonical_id_v1();
        let provider_id = identity.surface.provider_id();
        if projection.providers.iter().any(|provider| {
            provider.provider_id == provider_id
                && provider
                    .accounts
                    .iter()
                    .any(|account| account.canonical_account_id == canonical_id)
        }) {
            continue;
        }
        let state = UsageGenerationView {
            capability: entry.capability.clone(),
            generation: 0,
            phase: UsageRefreshPhase::Idle,
            snapshot: None,
            error: None,
            retry_at_epoch: None,
        };
        let mut account = account_for_view(&state, 0, entry)?;
        account.refresh_capabilities = routes.get(&canonical_id).cloned().unwrap_or_default();
        if let Some(provider) = projection
            .providers
            .iter_mut()
            .find(|provider| provider.provider_id == provider_id)
        {
            provider.accounts.push(account);
        } else {
            projection.providers.push(UsageProviderV2 {
                provider_id: provider_id.to_owned(),
                display_name: identity.surface.label().to_owned(),
                rank: 0,
                membership_state: UsageMembershipStateV2::Current,
                freshness: account.freshness.clone(),
                accounts: vec![account],
                issues: Vec::new(),
            });
        }
    }
    for old_provider in &previous.providers {
        for old_account in &old_provider.accounts {
            if projection.providers.iter().any(|provider| {
                provider.provider_id == old_provider.provider_id
                    && provider.accounts.iter().any(|account| {
                        account.canonical_account_id == old_account.canonical_account_id
                    })
            }) {
                continue;
            }
            let mut account = old_account.clone();
            mark_revoked(&mut account);
            account.refresh_capabilities.clear();
            if let Some(provider) = projection
                .providers
                .iter_mut()
                .find(|provider| provider.provider_id == old_provider.provider_id)
            {
                provider.accounts.push(account);
            } else {
                let mut provider = old_provider.clone();
                provider.accounts = vec![account];
                projection.providers.push(provider);
            }
        }
    }
    rank_providers(projection);
    Ok(())
}

fn rank_providers(projection: &mut UsageProjectionV2) {
    projection.providers.sort_by_key(|provider| {
        HostSurfaceId::ALL
            .iter()
            .position(|surface| surface.provider_id() == provider.provider_id)
            .unwrap_or(usize::MAX)
    });
    for (rank, provider) in projection.providers.iter_mut().enumerate() {
        provider.rank = u32::try_from(rank).unwrap_or(u32::MAX);
        provider.accounts.sort_by(|left, right| {
            left.display_label
                .cmp(&right.display_label)
                .then_with(|| left.canonical_account_id.cmp(&right.canonical_account_id))
        });
        for (rank, account) in provider.accounts.iter_mut().enumerate() {
            account.rank = u32::try_from(rank).unwrap_or(u32::MAX);
        }
        provider.freshness = aggregate_freshness(
            provider
                .accounts
                .iter()
                .any(|account| account.freshness.phase == UsageFreshnessPhaseV2::Refreshing),
            &provider.accounts,
        );
    }
}

fn mark_revoked(account: &mut UsageAccountV2) {
    account.status_label = Some("removed".to_owned());
    account.lifecycle = UsageLifecycleV2::Unavailable;
    account.freshness.phase = UsageFreshnessPhaseV2::Failed;
    account.freshness.is_stale = true;
    account.windows.clear();
    account.metric_groups.clear();
    account.issues.clear();
}

fn mark_cached_stale(account: &mut UsageAccountV2) {
    if account.lifecycle == UsageLifecycleV2::AgentUninitialized {
        account.lifecycle = UsageLifecycleV2::Available;
    }
    account.freshness.is_stale = true;
    if account.freshness.phase != UsageFreshnessPhaseV2::Refreshing {
        account.freshness.phase = UsageFreshnessPhaseV2::Stale;
    }
    for group in &mut account.metric_groups {
        group.is_stale = true;
        group.phase = UsageFreshnessPhaseV2::Stale;
    }
}

fn retain_last_good(account: &mut UsageAccountV2, previous: &UsageAccountV2) {
    if account.freshness.last_good_at_epoch.is_some()
        || previous.freshness.last_good_at_epoch.is_none()
        || previous.status_label.as_deref() == Some("removed")
    {
        return;
    }
    account.display_label = previous.display_label.clone();
    account.username = previous.username.clone();
    account.auth_origin = previous.auth_origin.clone();
    account.plan_label = previous.plan_label.clone();
    account.windows = previous.windows.clone();
    account.metric_groups = previous.metric_groups.clone();
    account.freshness.last_good_at_epoch = previous.freshness.last_good_at_epoch;
    mark_cached_stale(account);
}

fn catalog_entries(catalog: &AcceptedCatalog) -> Vec<UsageCatalogEntry> {
    catalog.values().cloned().collect()
}

/// Rebuild provider/account rows from per-account generation views.
///
/// Providers and accounts are rebuilt in settled `(surface_id, account_id)`
/// order with canonical ranks. Projection-level `unresolved`, `issues`, and
/// the catalog revision are preserved untouched.
fn merge_views(
    projection: &mut UsageProjectionV2,
    views: &[UsageGenerationView],
    catalog: &AcceptedCatalog,
) -> Result<(), String> {
    let routes = account_routes(catalog)?;
    // Construct a complete draft before changing the current publication.
    let mut ordered = views.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|view| {
        (
            view.snapshot
                .as_ref()
                .map(|snapshot| snapshot.fetched_at_epoch)
                .unwrap_or(i64::MIN),
            &view.capability,
        )
    });
    let mut accounts = BTreeMap::<(String, String), UsageAccountV2>::new();
    let mut unresolved = Vec::new();
    for view in ordered {
        let mut normalized = view.clone();
        let entry = catalog
            .get(&view.capability)
            .ok_or_else(|| "generation capability is outside accepted catalog".to_owned())?;
        if let Some(snapshot) = &mut normalized.snapshot {
            if snapshot.canonical_identity != entry.canonical_identity {
                return Err("generation logical identity differs from accepted catalog".to_owned());
            }
            if snapshot.account_identity.as_ref().is_none_or(|route| {
                route.account_id != entry.capability.account_id
                    || route.surface_id != entry.capability.surface_id
            }) {
                return Err("generation refresh route differs from accepted catalog".to_owned());
            }
            if snapshot.account_identity.as_ref().is_none_or(|route| {
                route.source_revision.as_deref() != Some(entry.revision.as_str())
            }) {
                let strong_identity = entry.canonical_identity.as_ref().is_some_and(|identity| matches!(identity.subject,
                    jackin_protocol::control::UsageCanonicalAccountSubject::ProviderId(_)
                        | jackin_protocol::control::UsageCanonicalAccountSubject::ProviderStableHandle(_)));
                if strong_identity {
                    if matches!(
                        snapshot.status,
                        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
                    ) {
                        snapshot.status = UsageSnapshotStatus::Stale;
                    }
                } else {
                    return Err(
                        "generation source revision differs from accepted catalog".to_owned()
                    );
                }
            }
            for bucket in &snapshot.buckets {
                bucket.validate_count_representation()?;
            }
        }
        let view = &normalized;
        let Some(identity) = catalog_identity(entry)? else {
            let issues = view
                .error
                .as_ref()
                .map(|error| UsageIssueV2 {
                    code: issue_code(error.kind),
                    scope: UsageIssueScopeV2::Provider,
                    recoverability: issue_recoverability(error.kind),
                    message: error.message.clone(),
                    retry_at_epoch: view.retry_at_epoch,
                })
                .into_iter()
                .collect();
            unresolved.push(UsageUnresolvedV2 {
                provider_id: HostSurfaceId::from_id(&view.capability.surface_id)
                    .ok_or_else(|| "unknown capability surface".to_owned())?
                    .provider_id()
                    .to_owned(),
                capability_id: view.capability.account_id.clone(),
                configuration_count: entry.provenance_count,
                state: view
                    .error
                    .as_ref()
                    .map_or(UsageLifecycleV2::NeedsLogin, |error| {
                        failure_lifecycle(error.kind)
                    }),
                issues,
            });
            continue;
        };
        let mut account = account_for_view(view, 0, entry)?;
        // The current projection has already passed accepted-catalog rotation
        // checks. Keep its last good observation while the current route has
        // no usable snapshot; never reconstruct a snapshot from presentation.
        if let Some(previous) = projection
            .providers
            .iter()
            .flat_map(|p| &p.accounts)
            .find(|old| old.canonical_account_id == account.canonical_account_id)
        {
            retain_last_good(&mut account, previous);
        }
        account.refresh_capabilities = routes
            .get(&account.canonical_account_id)
            .cloned()
            .unwrap_or_default();
        // Every route carries the same deduplicated logical provenance count.
        if account.refresh_capabilities.iter().any(|route| {
            catalog
                .get(route)
                .is_none_or(|other| other.provenance_count != account.provenance_count)
        }) {
            return Err("canonical aliases have inconsistent provenance count".to_owned());
        }
        let key = (
            identity.surface.provider_id().to_owned(),
            account.canonical_account_id.clone(),
        );
        if let Some(previous) = accounts.remove(&key) {
            retain_last_good(&mut account, &previous);
            for issue in previous.issues {
                if !account.issues.contains(&issue) {
                    account.issues.push(issue);
                }
            }
            account.freshness.retry_at_epoch = [
                account.freshness.retry_at_epoch,
                previous.freshness.retry_at_epoch,
            ]
            .into_iter()
            .flatten()
            .min();
            if previous.freshness.phase == UsageFreshnessPhaseV2::Refreshing {
                account.freshness.phase = UsageFreshnessPhaseV2::Refreshing;
            }
        }
        accounts.insert(key, account);
    }
    let mut providers = Vec::<UsageProviderV2>::new();
    for ((provider_id, _), account) in accounts {
        if let Some(provider) = providers
            .iter_mut()
            .find(|provider| provider.provider_id == provider_id)
        {
            provider.accounts.push(account);
        } else {
            let surface = HostSurfaceId::ALL
                .iter()
                .find(|surface| surface.provider_id() == provider_id)
                .ok_or_else(|| "unknown canonical provider".to_owned())?;
            providers.push(UsageProviderV2 {
                provider_id,
                display_name: surface.label().to_owned(),
                rank: 0,
                membership_state: UsageMembershipStateV2::Current,
                freshness: account.freshness.clone(),
                accounts: vec![account],
                issues: Vec::new(),
            });
        }
    }
    let mut draft = projection.clone();
    draft.providers = providers;
    draft.unresolved = unresolved;
    draft.refresh_state = if views.iter().any(|view| view.phase.is_active()) {
        UsageProjectionRefreshStateV2::Refreshing
    } else {
        UsageProjectionRefreshStateV2::Idle
    };
    rank_providers(&mut draft);
    draft.validate()?;
    *projection = draft;
    Ok(())
}

fn aggregate_freshness(any_active: bool, accounts: &[UsageAccountV2]) -> UsageFreshnessV2 {
    let mut freshness = UsageFreshnessV2 {
        generation: 0,
        phase: UsageFreshnessPhaseV2::Failed,
        last_good_at_epoch: None,
        retry_at_epoch: None,
        is_stale: false,
    };
    for account in accounts {
        freshness.generation = freshness.generation.max(account.freshness.generation);
        freshness.last_good_at_epoch = freshness
            .last_good_at_epoch
            .max(account.freshness.last_good_at_epoch);
        freshness.retry_at_epoch = [freshness.retry_at_epoch, account.freshness.retry_at_epoch]
            .into_iter()
            .flatten()
            .min();
        freshness.is_stale |= account.freshness.is_stale;
    }
    freshness.phase = if any_active {
        UsageFreshnessPhaseV2::Refreshing
    } else if accounts
        .iter()
        .all(|account| account.freshness.phase == UsageFreshnessPhaseV2::Failed)
    {
        UsageFreshnessPhaseV2::Failed
    } else if accounts
        .iter()
        .any(|account| account.freshness.phase == UsageFreshnessPhaseV2::Stale)
    {
        UsageFreshnessPhaseV2::Stale
    } else {
        UsageFreshnessPhaseV2::Current
    };
    freshness
}

fn account_for_view(
    view: &UsageGenerationView,
    rank: usize,
    entry: &UsageCatalogEntry,
) -> Result<UsageAccountV2, String> {
    if entry.capability != view.capability {
        return Err("account generation capability differs from catalog entry".to_owned());
    }
    let identity = catalog_identity(entry)?
        .ok_or_else(|| "unresolved capability has no canonical account".to_owned())?;
    let canonical_account_id = identity.canonical_id_v1();
    let mut snapshot = view.snapshot.clone();
    let header_label = snapshot
        .as_ref()
        .map(|snapshot| snapshot.account.account_label.clone())
        .unwrap_or_default();
    let provider_label = snapshot
        .as_ref()
        .map(|snapshot| snapshot.account.provider_label.clone())
        .unwrap_or_default();
    let display_label = if header_label.trim().is_empty() {
        if provider_label.trim().is_empty() {
            view.capability.surface_id.clone()
        } else {
            provider_label.clone()
        }
    } else {
        header_label.clone()
    };
    let lifecycle = snapshot.as_ref().map_or_else(
        || {
            view.error
                .as_ref()
                .map_or(UsageLifecycleV2::AgentUninitialized, |error| {
                    failure_lifecycle(error.kind)
                })
        },
        |snapshot| lifecycle(snapshot.status, snapshot.confidence),
    );
    let is_stale = snapshot.as_ref().is_some_and(|snapshot| {
        matches!(snapshot.status, UsageSnapshotStatus::Stale)
            || view.phase == UsageRefreshPhase::Failed
                && matches!(
                    snapshot.status,
                    UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
                )
    });
    let phase = if view.phase.is_active() {
        UsageFreshnessPhaseV2::Refreshing
    } else if !snapshot.as_ref().is_some_and(|snapshot| {
        matches!(
            snapshot.status,
            UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
        )
    }) {
        UsageFreshnessPhaseV2::Failed
    } else if is_stale {
        UsageFreshnessPhaseV2::Stale
    } else {
        UsageFreshnessPhaseV2::Current
    };
    if is_stale {
        if let Some(snapshot) = &mut snapshot {
            snapshot.status = UsageSnapshotStatus::Stale;
        }
    }
    let windows = snapshot
        .as_ref()
        .map(|snapshot| windows_for_snapshot(&canonical_account_id, snapshot))
        .transpose()?
        .unwrap_or_default();
    let metric_groups = snapshot
        .as_ref()
        .map(|snapshot| {
            metric_groups_for_view(
                &canonical_account_id,
                snapshot,
                snapshot.account.plan_label.as_deref(),
            )
        })
        .transpose()?
        .unwrap_or_default();
    let issues = view
        .error
        .as_ref()
        .map(|error| {
            vec![UsageIssueV2 {
                code: issue_code(error.kind),
                scope: UsageIssueScopeV2::Account,
                recoverability: issue_recoverability(error.kind),
                message: error.message.clone(),
                retry_at_epoch: view.retry_at_epoch,
            }]
        })
        .unwrap_or_default();
    Ok(UsageAccountV2 {
        canonical_account_id,
        refresh_capabilities: vec![entry.capability.clone()],
        identity_kind: canonical_identity_kind(&identity),
        rank: u32::try_from(rank).unwrap_or(u32::MAX),
        display_label,
        username: snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.account.username.clone()),
        auth_origin: snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.account.credential_origin.clone()),
        plan_label: snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.account.plan_label.clone()),
        status_label: None,
        lifecycle,
        freshness: UsageFreshnessV2 {
            generation: view.generation,
            phase,
            last_good_at_epoch: snapshot
                .as_ref()
                .filter(|snapshot| {
                    matches!(
                        snapshot.status,
                        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
                    )
                })
                .map(|snapshot| snapshot.fetched_at_epoch),
            retry_at_epoch: view.retry_at_epoch,
            is_stale,
        },
        provenance_count: entry.provenance_count,
        windows,
        metric_groups,
        credential_expires_at_epoch: None,
        issues,
    })
}

fn windows_for_snapshot(
    account_id: &str,
    snapshot: &FocusedUsageView,
) -> Result<Vec<UsageLimitWindowV2>, String> {
    snapshot
        .buckets
        .iter()
        .enumerate()
        .map(|(rank, bucket)| project_window(account_id, bucket, rank))
        .collect()
}

pub(in crate::host) fn issue_code(kind: UsageCoordinationErrorKind) -> String {
    match kind {
        UsageCoordinationErrorKind::Unavailable => "unavailable",
        UsageCoordinationErrorKind::Unauthorized => "unauthorized",
        UsageCoordinationErrorKind::CatalogRevoked => "catalog_revoked",
        UsageCoordinationErrorKind::CatalogRevisionConflict => "catalog_revision_conflict",
        UsageCoordinationErrorKind::OwnerLost => "owner_lost",
        UsageCoordinationErrorKind::WaitTimeout => "wait_timeout",
        UsageCoordinationErrorKind::CorruptState => "corrupt_state",
        UsageCoordinationErrorKind::ProviderTimeout => "provider_timeout",
        UsageCoordinationErrorKind::ProviderUnavailable => "provider_unavailable",
        UsageCoordinationErrorKind::NeedsSecret => "needs_secret",
        UsageCoordinationErrorKind::RateLimited => "rate_limited",
        UsageCoordinationErrorKind::ProtocolMismatch => "protocol_mismatch",
    }
    .to_owned()
}

fn publisher_unavailable() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::Unavailable,
        message: "usage projection publisher is unavailable".to_owned(),
    }
}

fn projection_store_error(error: StateStoreError) -> UsageCoordinationError {
    match error {
        StateStoreError::Unavailable => publisher_unavailable(),
        StateStoreError::Corrupt => UsageCoordinationError {
            kind: UsageCoordinationErrorKind::CorruptState,
            message: "usage broker projection state is corrupt".to_owned(),
        },
    }
}

fn first_publisher_rollback_error(
    projection: Result<(), StateStoreError>,
    coordinator: Result<(), UsageCoordinationError>,
) -> Result<(), UsageCoordinationError> {
    match (projection, coordinator) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), _) => Err(projection_store_error(error)),
        (Ok(()), Err(error)) => Err(error),
    }
}

fn preserve_publisher_error(
    primary: UsageCoordinationError,
    rollback: Result<(), UsageCoordinationError>,
) -> UsageCoordinationError {
    match rollback {
        Ok(()) => primary,
        Err(rollback) => UsageCoordinationError {
            kind: primary.kind,
            message: format!(
                "{}; publication rollback failed: {}",
                primary.message, rollback.message
            ),
        },
    }
}

fn publisher_corrupt_state() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::CorruptState,
        message: "usage broker catalog is invalid".to_owned(),
    }
}

fn catalog_revision_conflict() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::CatalogRevisionConflict,
        message: "usage broker catalog publication lease is stale".to_owned(),
    }
}

pub(in crate::host) const fn issue_recoverability(
    kind: UsageCoordinationErrorKind,
) -> UsageIssueRecoverabilityV2 {
    match kind {
        UsageCoordinationErrorKind::NeedsSecret | UsageCoordinationErrorKind::Unauthorized => {
            UsageIssueRecoverabilityV2::ActionRequired
        }
        UsageCoordinationErrorKind::ProtocolMismatch
        | UsageCoordinationErrorKind::CorruptState
        | UsageCoordinationErrorKind::OwnerLost
        | UsageCoordinationErrorKind::CatalogRevoked
        | UsageCoordinationErrorKind::CatalogRevisionConflict => {
            UsageIssueRecoverabilityV2::Terminal
        }
        _ => UsageIssueRecoverabilityV2::Retryable,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use jackin_protocol::control::{
        Money, QuotaBucketView, StatusSlot, UsageConfidence, UsageSeverity, UsageSource,
    };
    use jackin_protocol::usage_broker::{
        UsageAccountCapability, UsageFreshnessPhaseV2, UsageIdentityKindV2, UsageLifecycleV2,
        UsageMetricValueV2, UsagePercent, UsageProjectionRefreshStateV2, UsageProjectionSchemaV2,
        UsageQuotaStateV2,
    };

    use super::*;
    use crate::coordinator::{
        AccountStateEnvelope, AccountStateStore, ProviderProbeOutcome, StateStoreError,
        UsageCoordinatorConfig, UsageProviderExecutor,
    };

    #[derive(Default)]
    struct MemoryStore {
        states: Mutex<BTreeMap<UsageAccountCapability, AccountStateEnvelope>>,
    }

    impl AccountStateStore for MemoryStore {
        fn load(
            &self,
            capability: &UsageAccountCapability,
            _now_epoch: i64,
        ) -> Result<Option<AccountStateEnvelope>, StateStoreError> {
            Ok(self.states.lock().unwrap().get(capability).cloned())
        }

        fn store(
            &self,
            envelope: &AccountStateEnvelope,
            _now_epoch: i64,
        ) -> Result<(), StateStoreError> {
            self.states
                .lock()
                .unwrap()
                .insert(envelope.capability.clone(), envelope.clone());
            Ok(())
        }
    }

    struct ImmediateExecutor;

    #[derive(Default)]
    struct ExhaustionExecutor {
        probes: AtomicUsize,
        reconciles: AtomicUsize,
    }

    impl UsageProviderExecutor for ExhaustionExecutor {
        fn probe(
            &self,
            _capability: &UsageAccountCapability,
            _generation: u64,
        ) -> ProviderProbeOutcome {
            self.probes.fetch_add(1, Ordering::SeqCst);
            ProviderProbeOutcome::success(fresh_view())
        }

        fn reconcile_catalog(
            &self,
            _entries: &[UsageCatalogEntry],
        ) -> Result<(), UsageCoordinationError> {
            self.reconciles.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    impl UsageProviderExecutor for ImmediateExecutor {
        fn probe(
            &self,
            _capability: &UsageAccountCapability,
            _generation: u64,
        ) -> ProviderProbeOutcome {
            ProviderProbeOutcome::success(fresh_view())
        }
    }

    struct FailingCatalogExecutor {
        reconciles: AtomicUsize,
    }

    impl UsageProviderExecutor for FailingCatalogExecutor {
        fn probe(
            &self,
            _capability: &UsageAccountCapability,
            _generation: u64,
        ) -> ProviderProbeOutcome {
            ProviderProbeOutcome::success(fresh_view())
        }

        fn reconcile_catalog(
            &self,
            _entries: &[UsageCatalogEntry],
        ) -> Result<(), UsageCoordinationError> {
            self.reconciles.fetch_add(1, Ordering::SeqCst);
            Err(UsageCoordinationError {
                kind: UsageCoordinationErrorKind::ProviderUnavailable,
                message: "fixture catalog reconciliation failed".to_owned(),
            })
        }
    }

    fn capability() -> UsageAccountCapability {
        UsageAccountCapability {
            account_id: "account-a".to_owned(),
            surface_id: "claude".to_owned(),
        }
    }

    fn test_entry(capability: UsageAccountCapability, revision: String) -> UsageCatalogEntry {
        UsageCatalogEntry {
            canonical_identity: Some(jackin_protocol::control::UsageCanonicalAccountIdentity {
                surface_id: capability.surface_id.clone(),
                subject: jackin_protocol::control::UsageCanonicalAccountSubject::ProviderId(
                    format!("provider:{}", capability.account_id),
                ),
            }),
            capability,
            revision,
            provenance_count: 1,
        }
    }

    fn fixture_views_and_catalog(
        views: &[UsageGenerationView],
    ) -> (Vec<UsageGenerationView>, AcceptedCatalog) {
        let catalog = accepted_catalog(
            views
                .iter()
                .map(|view| test_entry(view.capability.clone(), "fixture".to_owned())),
        )
        .unwrap();
        let mut views = views.to_vec();
        for view in &mut views {
            if let Some(snapshot) = &mut view.snapshot {
                snapshot.canonical_identity = catalog[&view.capability].canonical_identity.clone();
                snapshot.account_identity = Some(jackin_protocol::control::UsageAccountIdentity {
                    account_id: view.capability.account_id.clone(),
                    surface_id: view.capability.surface_id.clone(),
                    source_revision: Some("fixture".to_owned()),
                });
            }
        }
        (views, catalog)
    }

    fn merge_fixture_views(
        projection: &mut UsageProjectionV2,
        views: &[UsageGenerationView],
    ) -> Result<(), String> {
        let (views, catalog) = fixture_views_and_catalog(views);
        merge_views(projection, &views, &catalog)
    }

    fn fresh_view() -> FocusedUsageView {
        let mut view = FocusedUsageView::unavailable("fixture", 1_000);
        view.focused_agent = Some("claude".to_owned());
        view.focused_provider = Some("Claude".to_owned());
        view.account.provider_label = "Anthropic".to_owned();
        view.account.account_label = "account@example.test".to_owned();
        view.status = UsageSnapshotStatus::Fresh;
        view.source = UsageSource::ProviderApi;
        view.confidence = UsageConfidence::Authoritative;
        view.buckets = vec![QuotaBucketView {
            count_quota: None,
            remaining_money: None,
            label: "Weekly".to_owned(),
            used_label: None,
            limit_label: None,
            remaining_percent: Some(75),
            reset_label: None,
            resets_at: None,
            status_slot: None,
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        }];
        view.last_error = None;
        view
    }

    fn empty_projection() -> UsageProjectionV2 {
        UsageProjectionV2 {
            schema_version: UsageProjectionSchemaV2,
            projection_id: "test:0".to_owned(),
            generated_at_epoch: 1_000,
            discovery_revision: "catalog".to_owned(),
            broker_instance_id: "test".to_owned(),
            broker_generation: 0,
            refresh_state: UsageProjectionRefreshStateV2::Idle,
            providers: Vec::new(),
            unresolved: Vec::new(),
            unresolved_grants: Vec::new(),
            issues: Vec::new(),
        }
    }

    #[test]
    fn catalog_reconciliation_publishes_removed_tombstone_atomically() {
        let temp = tempfile::tempdir().unwrap();
        let account = capability();
        let catalog_entry = test_entry(account.clone(), "credential-a".to_owned());
        let coordinator = Arc::new(UsageCoordinator::with_catalog(
            Arc::new(ImmediateExecutor),
            Arc::new(MemoryStore::default()),
            UsageCoordinatorConfig::default(),
            [catalog_entry.clone()],
        ));
        let projection = Arc::new(Mutex::new(empty_projection()));
        let store = FileProjectionStateStore::under_data_dir(temp.path());
        let publisher = ProjectionPublisher::new(
            Arc::clone(&coordinator),
            Arc::clone(&projection),
            store.clone(),
        )
        .with_catalog([catalog_entry.clone()])
        .unwrap();

        let queued = coordinator
            .request_refresh(&account, 0, true, 1_000)
            .unwrap();
        assert_eq!(
            coordinator
                .join_generation(&account, queued.generation, Duration::from_secs(1), 1_001)
                .unwrap()
                .phase,
            UsageRefreshPhase::Completed
        );
        publisher.observe(&account);
        assert!(publisher.publish_due(1_001));

        let removed = publisher
            .reconcile_catalog("catalog-2".to_owned(), Vec::new(), 1_002)
            .unwrap();
        let account_row = &removed.providers[0].accounts[0];
        assert_ne!(account_row.canonical_account_id, account.account_id);
        assert!(account_row.refresh_capabilities.is_empty());
        assert_eq!(account_row.display_label, "account@example.test");
        assert_eq!(account_row.status_label.as_deref(), Some("removed"));
        assert_eq!(account_row.lifecycle, UsageLifecycleV2::Unavailable);
        assert_eq!(account_row.freshness.phase, UsageFreshnessPhaseV2::Failed);
        assert_eq!(removed.discovery_revision, "catalog-2");

        let persisted = store.load().unwrap().unwrap();
        assert!(persisted.catalog.is_empty());
        assert_eq!(persisted.projection, removed);
    }

    fn account_with_retry(id: &str, retry_at_epoch: Option<i64>) -> UsageAccountV2 {
        UsageAccountV2 {
            canonical_account_id: id.to_owned(),
            refresh_capabilities: Vec::new(),
            username: None,
            auth_origin: None,
            identity_kind: UsageIdentityKindV2::ProviderAccountId,
            rank: 0,
            display_label: id.to_owned(),
            plan_label: None,
            status_label: None,
            lifecycle: UsageLifecycleV2::Available,
            freshness: UsageFreshnessV2 {
                generation: 1,
                phase: UsageFreshnessPhaseV2::Failed,
                last_good_at_epoch: None,
                retry_at_epoch,
                is_stale: false,
            },
            provenance_count: 1,
            windows: Vec::new(),
            metric_groups: Vec::new(),
            credential_expires_at_epoch: None,
            issues: Vec::new(),
        }
    }

    #[test]
    fn capsule_publication_preserves_identity_kind_and_provenance_per_account() {
        let first = UsageAccountCapability {
            account_id: "account-a".to_owned(),
            surface_id: "claude".to_owned(),
        };
        let second = UsageAccountCapability {
            account_id: "account-b".to_owned(),
            surface_id: "claude".to_owned(),
        };
        let views = vec![
            UsageGenerationView {
                capability: first.clone(),
                generation: 1,
                phase: UsageRefreshPhase::Completed,
                snapshot: Some(fresh_view()),
                error: None,
                retry_at_epoch: None,
            },
            UsageGenerationView {
                capability: second.clone(),
                generation: 1,
                phase: UsageRefreshPhase::Completed,
                snapshot: Some(fresh_view()),
                error: None,
                retry_at_epoch: None,
            },
        ];
        let mut first_entry = test_entry(first.clone(), "fixture".to_owned());
        first_entry.provenance_count = 3;
        let mut second_entry = test_entry(second.clone(), "fixture".to_owned());
        second_entry.canonical_identity.as_mut().unwrap().subject =
            jackin_protocol::control::UsageCanonicalAccountSubject::ProviderStableHandle(
                "second@example.test".to_owned(),
            );
        second_entry.provenance_count = 2;
        let catalog = accepted_catalog([first_entry, second_entry]).unwrap();
        let mut views = views;
        for view in &mut views {
            view.snapshot.as_mut().unwrap().canonical_identity =
                catalog[&view.capability].canonical_identity.clone();
            view.snapshot.as_mut().unwrap().account_identity =
                Some(jackin_protocol::control::UsageAccountIdentity {
                    account_id: view.capability.account_id.clone(),
                    surface_id: view.capability.surface_id.clone(),
                    source_revision: Some("fixture".to_owned()),
                });
        }
        let mut projection = empty_projection();

        merge_views(&mut projection, &views, &catalog).unwrap();

        let accounts = &projection.providers[0].accounts;
        assert_eq!(accounts.len(), 2);
        let first_account = accounts
            .iter()
            .find(|account| account.refresh_capabilities.contains(&first))
            .unwrap();
        let second_account = accounts
            .iter()
            .find(|account| account.refresh_capabilities.contains(&second))
            .unwrap();
        assert_eq!(
            first_account.identity_kind,
            UsageIdentityKindV2::ProviderAccountId
        );
        assert_eq!(first_account.provenance_count, 3);
        assert_eq!(
            second_account.identity_kind,
            UsageIdentityKindV2::ProviderStableHandle
        );
        assert_eq!(second_account.provenance_count, 2);
    }

    #[test]
    fn capsule_publication_preserves_openrouter_overage_raw_used_percent() {
        let capability = capability();
        let mut view = fresh_view();
        view.buckets = vec![QuotaBucketView {
            count_quota: None,
            remaining_money: None,
            label: "Account credits".to_owned(),
            used_label: Some("$120".to_owned()),
            limit_label: Some("$100".to_owned()),
            remaining_percent: None,
            reset_label: None,
            resets_at: None,
            status_slot: Some(StatusSlot::Spend),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: Some(Money::new(12_000, "USD", 2)),
            limit_money: Some(Money::new(10_000, "USD", 2)),
            severity: UsageSeverity::Danger,
        }];
        let views = [UsageGenerationView {
            capability,
            generation: 1,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(view),
            error: None,
            retry_at_epoch: None,
        }];
        let mut projection = empty_projection();

        merge_fixture_views(&mut projection, &views).unwrap();

        let window = &projection.providers[0].accounts[0].windows[0];
        assert_eq!(window.value_label, "120% used");
        assert_eq!(window.used_percent.map(UsagePercent::get), Some(100));
        assert_eq!(window.used_raw_percent, Some(120));
        assert_eq!(window.remaining_percent, None);
        assert_eq!(window.remaining_raw_percent, None);
        assert_eq!(window.quota_state, UsageQuotaStateV2::Exhausted);
        match &projection.providers[0].accounts[0].metric_groups[1].value {
            UsageMetricValueV2::SpendCap {
                cap,
                spent,
                remaining,
            } => {
                assert_eq!(cap, &Some(Money::new(10_000, "USD", 2)));
                assert_eq!(spent, &Some(Money::new(12_000, "USD", 2)));
                assert_eq!(remaining, &Some(Money::new(-20, "USD", 0)));
            }
            other => panic!("expected structured spend-cap value, got {other:?}"),
        }
        window.validate(0).unwrap();
    }

    #[test]
    fn publication_marks_empty_and_stale_quota_states_without_fabrication() {
        let capability = capability();
        let mut empty = fresh_view();
        empty.status = UsageSnapshotStatus::Fresh;
        empty.buckets = vec![QuotaBucketView {
            count_quota: None,
            remaining_money: None,
            label: "Provider-defined".to_owned(),
            used_label: None,
            limit_label: None,
            remaining_percent: None,
            reset_label: None,
            resets_at: None,
            status_slot: None,
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        }];
        let unknown_projection = {
            let views = [UsageGenerationView {
                capability: capability.clone(),
                generation: 1,
                phase: UsageRefreshPhase::Completed,
                snapshot: Some(empty),
                error: None,
                retry_at_epoch: None,
            }];
            let mut projection = empty_projection();
            merge_fixture_views(&mut projection, &views).unwrap();
            projection
        };
        assert_eq!(
            unknown_projection.providers[0].accounts[0].windows[0].quota_state,
            UsageQuotaStateV2::Unknown
        );

        let mut stale = fresh_view();
        stale.status = UsageSnapshotStatus::Stale;
        stale.buckets[0].status = UsageSnapshotStatus::Stale;
        let views = [UsageGenerationView {
            capability,
            generation: 2,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(stale),
            error: None,
            retry_at_epoch: None,
        }];
        let mut projection = empty_projection();
        merge_fixture_views(&mut projection, &views).unwrap();
        let account = &projection.providers[0].accounts[0];
        assert_eq!(account.freshness.phase, UsageFreshnessPhaseV2::Stale);
        assert!(account.freshness.is_stale);
        assert_eq!(account.windows[0].quota_state, UsageQuotaStateV2::Available);
    }

    #[test]
    fn publication_refreshing_is_scoped_to_provider_surface() {
        let stalled = UsageAccountCapability {
            account_id: "stalled".to_owned(),
            surface_id: "claude".to_owned(),
        };
        let healthy = UsageAccountCapability {
            account_id: "healthy".to_owned(),
            surface_id: "codex".to_owned(),
        };
        let views = [
            UsageGenerationView {
                capability: stalled,
                generation: 2,
                phase: UsageRefreshPhase::Updating,
                snapshot: None,
                error: None,
                retry_at_epoch: None,
            },
            UsageGenerationView {
                capability: healthy,
                generation: 1,
                phase: UsageRefreshPhase::Completed,
                snapshot: Some(fresh_view()),
                error: None,
                retry_at_epoch: None,
            },
        ];
        let mut projection = empty_projection();
        merge_fixture_views(&mut projection, &views).unwrap();

        assert_eq!(
            projection.refresh_state,
            UsageProjectionRefreshStateV2::Refreshing
        );
        assert_eq!(
            projection
                .providers
                .iter()
                .find(|provider| provider.provider_id == "anthropic")
                .map(|provider| provider.freshness.phase),
            Some(UsageFreshnessPhaseV2::Refreshing)
        );
        assert_eq!(
            projection
                .providers
                .iter()
                .find(|provider| provider.provider_id == "openai")
                .map(|provider| provider.freshness.phase),
            Some(UsageFreshnessPhaseV2::Current)
        );
    }

    #[test]
    fn retry_deadline_aggregation_is_independent_of_account_order() {
        let early = account_with_retry("early", Some(100));
        let late = account_with_retry("late", Some(200));
        let first = aggregate_freshness(false, &[late.clone(), early.clone()]);
        let second = aggregate_freshness(false, &[early, late]);
        assert_eq!(first.retry_at_epoch, Some(100));
        assert_eq!(second.retry_at_epoch, Some(100));
    }

    #[test]
    fn publication_checkpoint_advances_only_after_durable_store() {
        let temp = tempfile::tempdir().unwrap();
        let account = capability();
        let entry = test_entry(account.clone(), "fixture".to_owned());
        let coordinator = Arc::new(UsageCoordinator::with_catalog(
            Arc::new(ImmediateExecutor),
            Arc::new(MemoryStore::default()),
            UsageCoordinatorConfig::default(),
            [entry.clone()],
        ));
        let queued = coordinator
            .request_refresh(&account, 0, true, 1_000)
            .unwrap();
        coordinator
            .join_generation(&account, queued.generation, Duration::from_secs(2), 1_001)
            .unwrap();

        let projection = Arc::new(Mutex::new(empty_projection()));
        let publisher = ProjectionPublisher::new(
            Arc::clone(&coordinator),
            Arc::clone(&projection),
            FileProjectionStateStore::under_data_dir(temp.path()),
        )
        .with_catalog([entry])
        .unwrap();
        publisher.observe(&account);

        let broker_dir = temp.path().join("usage-broker");
        fs::create_dir_all(&broker_dir).unwrap();
        fs::create_dir(broker_dir.join("projection.json")).unwrap();
        assert!(!publisher.publish_due(1_002));
        assert_eq!(projection.lock().unwrap().broker_generation, 0);

        fs::remove_dir(broker_dir.join("projection.json")).unwrap();
        assert!(publisher.publish_due(1_003));
        assert_eq!(projection.lock().unwrap().broker_generation, 1);
        assert!(!publisher.publish_due(1_004));
    }

    #[test]
    fn catalog_publication_preserves_stable_inventory_without_observing_new_routes() {
        let temp = tempfile::tempdir().unwrap();
        let account_a = capability();
        let account_b = UsageAccountCapability {
            account_id: "account-b".to_owned(),
            surface_id: "claude".to_owned(),
        };
        let coordinator = Arc::new(UsageCoordinator::with_catalog(
            Arc::new(ImmediateExecutor),
            Arc::new(MemoryStore::default()),
            UsageCoordinatorConfig::default(),
            [
                test_entry(account_a.clone(), "revision-a".to_owned()),
                test_entry(account_b.clone(), "revision-b".to_owned()),
            ],
        ));
        let projection = Arc::new(Mutex::new(empty_projection()));
        let publisher = ProjectionPublisher::new(
            Arc::clone(&coordinator),
            Arc::clone(&projection),
            FileProjectionStateStore::under_data_dir(temp.path()),
        )
        .with_catalog([
            test_entry(account_a.clone(), "revision-a".to_owned()),
            test_entry(account_b.clone(), "revision-b".to_owned()),
        ])
        .unwrap();

        let queued = coordinator
            .request_refresh(&account_a, 0, true, 1_000)
            .unwrap();
        coordinator
            .join_generation(&account_a, queued.generation, Duration::from_secs(2), 1_001)
            .unwrap();
        publisher.observe(&account_a);
        assert!(publisher.publish_due(1_002));
        assert_eq!(projection.lock().unwrap().providers[0].accounts.len(), 2);
        let old_id = projection.lock().unwrap().providers[0]
            .accounts
            .iter()
            .find(|account| account.refresh_capabilities.contains(&account_a))
            .unwrap()
            .canonical_account_id
            .clone();

        let current = publisher
            .reconcile_catalog(
                "catalog-2".to_owned(),
                vec![test_entry(account_b.clone(), "revision-b".to_owned())],
                1_003,
            )
            .unwrap();
        let removed = current
            .providers
            .iter()
            .flat_map(|provider| provider.accounts.iter())
            .find(|account| account.canonical_account_id == old_id)
            .expect("removed account remains visible as a tombstone");
        assert_eq!(removed.status_label.as_deref(), Some("removed"));
        assert_eq!(removed.lifecycle, UsageLifecycleV2::Unavailable);
        assert!(removed.refresh_capabilities.is_empty());
        assert!(publisher.known_capabilities().is_empty());
        let persisted = FileProjectionStateStore::under_data_dir(temp.path())
            .load()
            .unwrap()
            .unwrap();
        assert_eq!(persisted.catalog.len(), 1);
        assert_eq!(persisted.catalog[0].capability, account_b);

        let reintroduced = publisher
            .reconcile_catalog(
                "catalog-3".to_owned(),
                vec![test_entry(account_a, "revision-a".to_owned())],
                1_004,
            )
            .unwrap();
        let restored = reintroduced
            .providers
            .iter()
            .flat_map(|provider| &provider.accounts)
            .find(|account| account.canonical_account_id == old_id)
            .unwrap();
        assert_eq!(restored.status_label.as_deref(), Some("Not refreshed"));
        assert_eq!(restored.refresh_capabilities.len(), 1);
        assert_eq!(restored.freshness.generation, 0);
    }

    #[test]
    fn same_capability_revision_purges_stale_published_quota() {
        let temp = tempfile::tempdir().unwrap();
        let account = capability();
        let mut old = test_entry(account.clone(), "credential-a".to_owned());
        old.canonical_identity.as_mut().unwrap().subject =
            jackin_protocol::control::UsageCanonicalAccountSubject::SourceCapability(
                "source-fixture".to_owned(),
            );
        let coordinator = Arc::new(UsageCoordinator::with_catalog(
            Arc::new(ImmediateExecutor),
            Arc::new(MemoryStore::default()),
            UsageCoordinatorConfig::default(),
            [old.clone()],
        ));
        let projection = Arc::new(Mutex::new(empty_projection()));
        let publisher = ProjectionPublisher::new(
            Arc::clone(&coordinator),
            Arc::clone(&projection),
            FileProjectionStateStore::under_data_dir(temp.path()),
        )
        .with_catalog([old.clone()])
        .unwrap();

        let generation = coordinator
            .request_refresh(&account, 0, true, 1_000)
            .unwrap()
            .generation;
        coordinator
            .join_generation(&account, generation, Duration::from_secs(1), 1_001)
            .unwrap();
        publisher.observe(&account);
        assert!(publisher.publish_due(1_001));

        let current = publisher
            .reconcile_catalog(
                "catalog".to_owned(),
                vec![UsageCatalogEntry {
                    revision: "credential-b".to_owned(),
                    ..old
                }],
                1_002,
            )
            .unwrap();
        let row = &current.providers[0].accounts[0];
        assert_eq!(row.status_label.as_deref(), Some("Not refreshed"));
        assert!(row.windows.is_empty());
        assert!(row.metric_groups.is_empty());
        let reset = coordinator.current(&account, 1_002).unwrap();
        assert_eq!(reset.phase, UsageRefreshPhase::Idle);
        assert!(reset.snapshot.is_none());
    }

    #[test]
    fn failed_catalog_executor_rolls_back_projection_and_catalog() {
        let temp = tempfile::tempdir().unwrap();
        let account = capability();
        let old = test_entry(account.clone(), "credential-a".to_owned());
        let executor = Arc::new(FailingCatalogExecutor {
            reconciles: AtomicUsize::new(0),
        });
        let broker_executor = Arc::clone(&executor);
        let broker_executor: Arc<dyn UsageProviderExecutor> = broker_executor;
        let coordinator = Arc::new(UsageCoordinator::with_catalog(
            broker_executor,
            Arc::new(MemoryStore::default()),
            UsageCoordinatorConfig::default(),
            [old.clone()],
        ));
        let projection = Arc::new(Mutex::new(empty_projection()));
        let store = FileProjectionStateStore::under_data_dir(temp.path());
        let publisher = ProjectionPublisher::new(
            Arc::clone(&coordinator),
            Arc::clone(&projection),
            store.clone(),
        )
        .with_catalog([old.clone()])
        .unwrap();

        let error = publisher
            .reconcile_catalog("new-catalog".to_owned(), Vec::new(), 1_001)
            .unwrap_err();
        assert_eq!(error.kind, UsageCoordinationErrorKind::ProviderUnavailable);
        assert_eq!(executor.reconciles.load(Ordering::SeqCst), 2);
        assert_eq!(projection.lock().unwrap().discovery_revision, "catalog");
        assert_eq!(
            publisher.known_capabilities(),
            Vec::<UsageAccountCapability>::new()
        );
        assert!(
            store.load().unwrap().is_none(),
            "executor rejection must not create a durable projection"
        );
        assert_eq!(coordinator.current(&account, 1_001).unwrap().generation, 0);
    }

    #[test]
    fn durable_projection_failure_does_not_activate_new_executor_catalog() {
        let temp = tempfile::tempdir().unwrap();
        let broker_dir = temp.path().join("usage-broker");
        fs::create_dir_all(&broker_dir).unwrap();
        fs::create_dir(broker_dir.join("projection.json")).unwrap();
        let account = capability();
        let old = test_entry(account.clone(), "credential-a".to_owned());
        let executor = Arc::new(FailingCatalogExecutor {
            reconciles: AtomicUsize::new(0),
        });
        let broker_executor = Arc::clone(&executor);
        let broker_executor: Arc<dyn UsageProviderExecutor> = broker_executor;
        let coordinator = Arc::new(UsageCoordinator::with_catalog(
            broker_executor,
            Arc::new(MemoryStore::default()),
            UsageCoordinatorConfig::default(),
            [old.clone()],
        ));
        let projection = Arc::new(Mutex::new(empty_projection()));
        let publisher = ProjectionPublisher::new(
            Arc::clone(&coordinator),
            Arc::clone(&projection),
            FileProjectionStateStore::under_data_dir(temp.path()),
        )
        .with_catalog([old])
        .unwrap();

        let error = publisher
            .reconcile_catalog("new-catalog".to_owned(), Vec::new(), 1_001)
            .unwrap_err();
        assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
        assert_eq!(executor.reconciles.load(Ordering::SeqCst), 0);
        assert_eq!(projection.lock().unwrap().discovery_revision, "catalog");
    }
    #[test]
    fn account_for_view_preserves_canonical_access_lifecycles() {
        for (status, expected) in [
            (
                UsageSnapshotStatus::NeedsLogin,
                UsageLifecycleV2::NeedsLogin,
            ),
            (
                UsageSnapshotStatus::NeedsSecret,
                UsageLifecycleV2::NeedsSecret,
            ),
        ] {
            let mut snapshot = fresh_view();
            snapshot.status = status;
            let account = account_for_view(
                &UsageGenerationView {
                    capability: capability(),
                    generation: 1,
                    phase: UsageRefreshPhase::Completed,
                    snapshot: Some(snapshot),
                    error: None,
                    retry_at_epoch: None,
                },
                0,
                &test_entry(capability(), "fixture".to_owned()),
            )
            .unwrap();
            assert_eq!(account.lifecycle, expected);
        }
        for (kind, expected) in [
            (
                UsageCoordinationErrorKind::Unauthorized,
                UsageLifecycleV2::NeedsLogin,
            ),
            (
                UsageCoordinationErrorKind::NeedsSecret,
                UsageLifecycleV2::NeedsSecret,
            ),
            (
                UsageCoordinationErrorKind::ProtocolMismatch,
                UsageLifecycleV2::Unsupported,
            ),
        ] {
            let account = account_for_view(
                &UsageGenerationView {
                    capability: capability(),
                    generation: 1,
                    phase: UsageRefreshPhase::Failed,
                    snapshot: None,
                    error: Some(UsageCoordinationError {
                        kind,
                        message: "Account access unavailable".to_owned(),
                    }),
                    retry_at_epoch: None,
                },
                0,
                &test_entry(capability(), "fixture".to_owned()),
            )
            .unwrap();
            assert_eq!(account.lifecycle, expected);
        }
    }
    #[test]
    fn publication_rejects_mixed_count_and_money_before_mutation() {
        use jackin_protocol::control::{
            CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
        };
        let mut snapshot = fresh_view();
        snapshot.buckets[0].count_quota = Some(CountQuota {
            used: Some(9),
            limit: Some(10),
            remaining: Some(1),
            unit: CountQuotaUnit::Requests,
            period: CountQuotaPeriod::UtcDaily,
            provenance: CountQuotaProvenance::ProviderReported,
        });
        snapshot.buckets[0].used_money = Some(Money::new(1, "USD", 2));
        let views = [UsageGenerationView {
            capability: capability(),
            generation: 1,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(snapshot),
            error: None,
            retry_at_epoch: None,
        }];
        let mut projection = empty_projection();
        let previous = projection.clone();
        let error = merge_fixture_views(&mut projection, &views).unwrap_err();
        assert_eq!(
            error,
            "quota bucket Weekly mixes count and money representations"
        );
        assert_eq!(projection, previous);
    }
    #[test]
    fn publication_preserves_exact_count_and_daily_period() {
        use jackin_protocol::control::{
            CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
        };
        use jackin_protocol::usage_broker::{
            UsageCalendarPeriodV2, UsageMetricPeriodV2, UsageMetricValueV2,
        };
        let count = CountQuota {
            used: Some(999),
            limit: Some(1_000),
            remaining: Some(1),
            unit: CountQuotaUnit::Requests,
            period: CountQuotaPeriod::UtcDaily,
            provenance: CountQuotaProvenance::ProviderReported,
        };
        let mut snapshot = fresh_view();
        snapshot.buckets[0].count_quota = Some(count.clone());
        snapshot.buckets[0].remaining_percent = Some(99);
        snapshot.buckets[0].status_slot = Some(StatusSlot::Weekly);
        let views = [UsageGenerationView {
            capability: capability(),
            generation: 1,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(snapshot),
            error: None,
            retry_at_epoch: None,
        }];
        let mut projection = empty_projection();
        merge_fixture_views(&mut projection, &views).unwrap();
        let account = &projection.providers[0].accounts[0];
        let window = &account.windows[0];
        assert_eq!(window.count_quota, Some(count.clone()));
        assert_eq!(window.remaining_percent.map(UsagePercent::get), Some(0));
        assert_eq!(window.quota_state, UsageQuotaStateV2::Available);
        assert_eq!(
            window.value_label,
            "999 / 1000 requests used · 1 requests left"
        );
        assert!(
            matches!(&account.metric_groups[0].value, UsageMetricValueV2::Window {
            count_quota: Some(actual), period: UsageMetricPeriodV2::Calendar { granularity: UsageCalendarPeriodV2::Daily }, unit: Some(unit), ..
        } if actual == &count && unit == "requests")
        );
        projection.validate().unwrap();
    }
    fn stable_entry(route: &str, revision: &str) -> UsageCatalogEntry {
        UsageCatalogEntry {
            capability: UsageAccountCapability {
                account_id: route.to_owned(),
                surface_id: "claude".to_owned(),
            },
            canonical_identity: Some(jackin_protocol::control::UsageCanonicalAccountIdentity {
                surface_id: "claude".to_owned(),
                subject: jackin_protocol::control::UsageCanonicalAccountSubject::ProviderId(
                    "stable-provider-id".to_owned(),
                ),
            }),
            provenance_count: 3,
            revision: revision.to_owned(),
        }
    }

    #[test]
    fn canonical_identity_survives_route_aliases_and_preserves_account_metadata() {
        let first = stable_entry("opaque-route-old", "credential-old");
        let second = stable_entry("opaque-route-new", "credential-new");
        let catalog = accepted_catalog([first.clone(), second.clone()]).unwrap();
        let mut snapshot = fresh_view();
        snapshot.canonical_identity = first.canonical_identity.clone();
        snapshot.account.account_label = "Display label does not identify the account".to_owned();
        snapshot.account.username = Some("donbeave".to_owned());
        snapshot.account.credential_origin = Some("API token · env CLAUDE_TOKEN".to_owned());
        snapshot.account_identity = Some(jackin_protocol::control::UsageAccountIdentity {
            account_id: "opaque-route-old".to_owned(),
            surface_id: "claude".to_owned(),
            source_revision: Some("credential-old".to_owned()),
        });
        let mut second_snapshot = snapshot.clone();
        second_snapshot.account_identity = Some(jackin_protocol::control::UsageAccountIdentity {
            account_id: "opaque-route-new".to_owned(),
            surface_id: "claude".to_owned(),
            source_revision: Some("credential-new".to_owned()),
        });
        let views = [
            UsageGenerationView {
                capability: first.capability.clone(),
                generation: 4,
                phase: UsageRefreshPhase::Completed,
                snapshot: Some(snapshot.clone()),
                error: None,
                retry_at_epoch: None,
            },
            UsageGenerationView {
                capability: second.capability.clone(),
                generation: 1,
                phase: UsageRefreshPhase::Updating,
                snapshot: Some(second_snapshot),
                error: None,
                retry_at_epoch: None,
            },
        ];
        let mut projection = empty_projection();
        merge_views(&mut projection, &views, &catalog).unwrap();
        let account = &projection.providers[0].accounts[0];
        assert_eq!(projection.providers[0].provider_id, "anthropic");
        assert_eq!(projection.providers[0].accounts.len(), 1);
        assert_eq!(
            account.canonical_account_id,
            "sha256:619939c9553ce44e3c95bd4f80eeb3b0b824167d3ded5f0e9a1767688748a522"
        );
        assert_eq!(
            account.refresh_capabilities,
            vec![second.capability, first.capability]
        );
        assert_eq!(account.provenance_count, 3);
        assert_eq!(account.username.as_deref(), Some("donbeave"));
        assert_eq!(
            account.auth_origin.as_deref(),
            Some("API token · env CLAUDE_TOKEN")
        );
        assert_eq!(account.freshness.phase, UsageFreshnessPhaseV2::Refreshing);
    }

    #[test]
    fn accepted_catalog_rotation_preserves_stable_identity_and_durable_full_proof() {
        let temp = tempfile::tempdir().unwrap();
        let old = stable_entry("opaque-route-old", "credential-old");
        let new = stable_entry("opaque-route-new", "credential-new");
        let coordinator = Arc::new(UsageCoordinator::with_catalog(
            Arc::new(ImmediateExecutor),
            Arc::new(MemoryStore::default()),
            UsageCoordinatorConfig::default(),
            [old.clone()],
        ));
        let projection = Arc::new(Mutex::new(empty_projection()));
        let store = FileProjectionStateStore::under_data_dir(temp.path());
        let publisher = ProjectionPublisher::new(
            Arc::clone(&coordinator),
            Arc::clone(&projection),
            store.clone(),
        )
        .with_catalog([old.clone()])
        .unwrap();
        let generation = coordinator
            .request_refresh(&old.capability, 0, true, 1_000)
            .unwrap()
            .generation;
        coordinator
            .join_generation(&old.capability, generation, Duration::from_secs(1), 1_001)
            .unwrap();
        publisher.observe(&old.capability);
        assert!(publisher.publish_due(1_001));
        let current = publisher
            .reconcile_catalog("catalog-new".to_owned(), vec![new.clone()], 1_002)
            .unwrap();
        let account = &current.providers[0].accounts[0];
        assert_eq!(
            account.canonical_account_id,
            "sha256:619939c9553ce44e3c95bd4f80eeb3b0b824167d3ded5f0e9a1767688748a522"
        );
        assert_eq!(account.refresh_capabilities, vec![new.capability.clone()]);
        assert_eq!(account.windows.len(), 1);
        assert_eq!(account.freshness.phase, UsageFreshnessPhaseV2::Stale);
        assert!(account.freshness.is_stale);
        assert_eq!(account.freshness.last_good_at_epoch, Some(1_000));
        assert!(account.metric_groups.iter().all(|group| group.is_stale));
        assert_eq!(account.provenance_count, 3);
        let durable = store.load().unwrap().unwrap();
        assert_eq!(durable.catalog, vec![new.clone()]);
        assert_eq!(durable.projection, current);
        let restarted = ProjectionPublisher::new(coordinator, projection, store)
            .with_catalog(durable.catalog)
            .unwrap();
        assert_eq!(restarted.known_capabilities(), vec![new.capability.clone()]);
        assert_eq!(
            restarted.current_projection().unwrap().providers[0].accounts[0].refresh_capabilities,
            vec![new.capability]
        );
    }

    #[test]
    fn forged_generation_identity_is_rejected_before_publication() {
        let entry = stable_entry("opaque-route", "credential");
        let catalog = accepted_catalog([entry.clone()]).unwrap();
        let mut snapshot = fresh_view();
        snapshot.canonical_identity =
            Some(jackin_protocol::control::UsageCanonicalAccountIdentity {
                surface_id: "claude".to_owned(),
                subject: jackin_protocol::control::UsageCanonicalAccountSubject::ProviderId(
                    "forged-subject".to_owned(),
                ),
            });
        let views = [UsageGenerationView {
            capability: entry.capability,
            generation: 1,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(snapshot),
            error: None,
            retry_at_epoch: None,
        }];
        let mut projection = empty_projection();
        let previous = projection.clone();
        assert_eq!(
            merge_views(&mut projection, &views, &catalog).unwrap_err(),
            "generation logical identity differs from accepted catalog"
        );
        assert_eq!(projection, previous);
    }

    #[test]
    fn source_identity_retains_quota_only_across_unchanged_revision() {
        for (revision, retained) in [("source-revision-a", true), ("source-revision-b", false)] {
            let mut old = stable_entry("source-route-old", "source-revision-a");
            old.canonical_identity.as_mut().unwrap().subject =
                jackin_protocol::control::UsageCanonicalAccountSubject::SourceCapability(
                    "authenticated-source".to_owned(),
                );
            let mut new = old.clone();
            new.capability.account_id = "source-route-new".to_owned();
            new.revision = revision.to_owned();
            let previous_catalog = accepted_catalog([old.clone()]).unwrap();
            let catalog = accepted_catalog([new.clone()]).unwrap();
            let mut snapshot = fresh_view();
            snapshot.canonical_identity = old.canonical_identity.clone();
            snapshot.account_identity = Some(jackin_protocol::control::UsageAccountIdentity {
                account_id: "source-route-old".to_owned(),
                surface_id: "claude".to_owned(),
                source_revision: Some("source-revision-a".to_owned()),
            });
            let mut projection = empty_projection();
            merge_views(
                &mut projection,
                &[UsageGenerationView {
                    capability: old.capability,
                    generation: 7,
                    phase: UsageRefreshPhase::Completed,
                    snapshot: Some(snapshot),
                    error: None,
                    retry_at_epoch: None,
                }],
                &previous_catalog,
            )
            .unwrap();
            let previous = projection.clone();
            // A persisted presentation discriminator cannot strengthen the
            // accepted source identity and preserve rotated credentials.
            projection.providers[0].accounts[0].identity_kind =
                UsageIdentityKindV2::ProviderAccountId;
            retain_revoked_accounts(
                &mut projection,
                &previous,
                &catalog,
                Some(&previous_catalog),
            )
            .unwrap();
            merge_views(
                &mut projection,
                &[UsageGenerationView {
                    capability: new.capability.clone(),
                    generation: 8,
                    phase: UsageRefreshPhase::Updating,
                    snapshot: None,
                    error: None,
                    retry_at_epoch: Some(1_200),
                }],
                &catalog,
            )
            .unwrap();
            let account = &projection.providers[0].accounts[0];
            assert_eq!(account.identity_kind, UsageIdentityKindV2::SourceCapability);
            assert_eq!(account.refresh_capabilities, vec![new.capability]);
            assert_eq!(account.freshness.generation, 8);
            assert_eq!(account.freshness.phase, UsageFreshnessPhaseV2::Refreshing);
            assert_eq!(account.freshness.retry_at_epoch, Some(1_200));
            assert_eq!(account.windows.len(), usize::from(retained));
            assert_eq!(
                account.freshness.last_good_at_epoch,
                retained.then_some(1_000)
            );
            assert_eq!(account.freshness.is_stale, retained);
        }
    }

    #[test]
    fn stale_source_revision_is_rejected_before_publication() {
        let mut entry = stable_entry("source-route", "source-revision-new");
        entry.canonical_identity.as_mut().unwrap().subject =
            jackin_protocol::control::UsageCanonicalAccountSubject::SourceCapability(
                "authenticated-source".to_owned(),
            );
        let catalog = accepted_catalog([entry.clone()]).unwrap();
        let mut snapshot = fresh_view();
        snapshot.canonical_identity = entry.canonical_identity;
        snapshot.account_identity = Some(jackin_protocol::control::UsageAccountIdentity {
            account_id: "source-route".to_owned(),
            surface_id: "claude".to_owned(),
            source_revision: Some("source-revision-old".to_owned()),
        });
        let mut projection = empty_projection();
        let previous = projection.clone();
        assert_eq!(
            merge_views(
                &mut projection,
                &[UsageGenerationView {
                    capability: entry.capability,
                    generation: 7,
                    phase: UsageRefreshPhase::Completed,
                    snapshot: Some(snapshot),
                    error: None,
                    retry_at_epoch: None,
                }],
                &catalog
            )
            .unwrap_err(),
            "generation source revision differs from accepted catalog"
        );
        assert_eq!(projection, previous);
    }

    #[test]
    fn strong_principal_requires_exact_route_and_marks_old_revision_stale() {
        let entry = stable_entry("current-route", "revision-new");
        let catalog = accepted_catalog([entry.clone()]).unwrap();
        for route in [None, Some("wrong-route"), Some("current-route")] {
            let mut snapshot = fresh_view();
            snapshot.canonical_identity = entry.canonical_identity.clone();
            snapshot.account_identity =
                route.map(|route| jackin_protocol::control::UsageAccountIdentity {
                    account_id: route.to_owned(),
                    surface_id: "claude".to_owned(),
                    source_revision: Some("revision-old".to_owned()),
                });
            let mut projection = empty_projection();
            let result = merge_views(
                &mut projection,
                &[UsageGenerationView {
                    capability: entry.capability.clone(),
                    generation: 7,
                    phase: UsageRefreshPhase::Completed,
                    snapshot: Some(snapshot),
                    error: None,
                    retry_at_epoch: None,
                }],
                &catalog,
            );
            if route == Some("current-route") {
                result.unwrap();
                let account = &projection.providers[0].accounts[0];
                assert_eq!(account.freshness.phase, UsageFreshnessPhaseV2::Stale);
                assert_eq!(account.freshness.last_good_at_epoch, Some(1_000));
                assert!(account.freshness.is_stale);
                assert_eq!(account.windows.len(), 1);
                assert!(account.metric_groups.iter().all(|group| group.is_stale));
            } else {
                assert_eq!(
                    result.unwrap_err(),
                    "generation refresh route differs from accepted catalog"
                );
                assert!(projection.providers.is_empty());
            }
        }
    }

    #[test]
    fn unresolved_observation_cannot_mint_account_from_display_metadata() {
        let entry = UsageCatalogEntry {
            capability: capability(),
            canonical_identity: None,
            provenance_count: 1,
            revision: "credential".to_owned(),
        };
        let catalog = accepted_catalog([entry.clone()]).unwrap();
        let mut snapshot = fresh_view();
        snapshot.account_identity = Some(jackin_protocol::control::UsageAccountIdentity {
            account_id: "account-a".to_owned(),
            surface_id: "claude".to_owned(),
            source_revision: Some("credential".to_owned()),
        });
        let views = [UsageGenerationView {
            capability: entry.capability,
            generation: 1,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(snapshot),
            error: None,
            retry_at_epoch: None,
        }];
        let mut projection = empty_projection();
        merge_views(&mut projection, &views, &catalog).unwrap();
        assert!(projection.providers.is_empty());
        assert_eq!(projection.unresolved.len(), 1);
        assert_eq!(projection.unresolved[0].provider_id, "anthropic");
        assert_eq!(projection.unresolved[0].configuration_count, 1);
    }

    #[test]
    fn restored_projection_cannot_forge_identity_kind_provenance_or_route_authority() {
        for mutation in ["kind", "count", "routes"] {
            let temp = tempfile::tempdir().unwrap();
            let mut entry = stable_entry("source-route", "source-revision");
            entry.canonical_identity.as_mut().unwrap().subject =
                jackin_protocol::control::UsageCanonicalAccountSubject::SourceCapability(
                    "authenticated-source".to_owned(),
                );
            let catalog = accepted_catalog([entry.clone()]).unwrap();
            let mut snapshot = fresh_view();
            snapshot.canonical_identity = entry.canonical_identity.clone();
            snapshot.account_identity = Some(jackin_protocol::control::UsageAccountIdentity {
                account_id: "source-route".to_owned(),
                surface_id: "claude".to_owned(),
                source_revision: Some("source-revision".to_owned()),
            });
            let mut projection = empty_projection();
            merge_views(
                &mut projection,
                &[UsageGenerationView {
                    capability: entry.capability.clone(),
                    generation: 7,
                    phase: UsageRefreshPhase::Completed,
                    snapshot: Some(snapshot),
                    error: None,
                    retry_at_epoch: None,
                }],
                &catalog,
            )
            .unwrap();
            let account = &mut projection.providers[0].accounts[0];
            match mutation {
                "kind" => account.identity_kind = UsageIdentityKindV2::ProviderAccountId,
                "count" => account.provenance_count = 99,
                "routes" => account.refresh_capabilities.clear(),
                _ => unreachable!(),
            }
            let coordinator = Arc::new(UsageCoordinator::with_catalog(
                Arc::new(ImmediateExecutor),
                Arc::new(MemoryStore::default()),
                UsageCoordinatorConfig::default(),
                [entry.clone()],
            ));
            let publisher = ProjectionPublisher::new(
                coordinator,
                Arc::new(Mutex::new(projection)),
                FileProjectionStateStore::under_data_dir(temp.path()),
            );
            assert!(
                publisher.with_catalog([entry]).is_err(),
                "forged {mutation} must fail admission"
            );
        }
    }

    #[test]
    fn unresolved_inventory_preserves_equal_route_ids_on_distinct_surfaces() {
        let catalog = accepted_catalog(["claude", "codex"].map(|surface| UsageCatalogEntry {
            capability: UsageAccountCapability {
                account_id: "shared-opaque-route".to_owned(),
                surface_id: surface.to_owned(),
            },
            canonical_identity: None,
            provenance_count: 0,
            revision: "revision".to_owned(),
        }))
        .unwrap();
        let mut projection = empty_projection();
        let previous = projection.clone();
        retain_revoked_accounts(&mut projection, &previous, &catalog, None).unwrap();
        assert_eq!(projection.unresolved.len(), 2);
        assert_eq!(
            projection
                .unresolved
                .iter()
                .map(|row| (row.provider_id.as_str(), row.capability_id.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("anthropic", "shared-opaque-route"),
                ("openai", "shared-opaque-route")
            ]
        );
        let mut changed = catalog.clone();
        changed
            .values_mut()
            .for_each(|entry| entry.provenance_count = 2);
        let previous = projection.clone();
        retain_revoked_accounts(&mut projection, &previous, &changed, Some(&catalog)).unwrap();
        assert!(
            projection
                .unresolved
                .iter()
                .all(|row| row.configuration_count == 2)
        );
    }

    #[test]
    fn restored_unresolved_inventory_requires_exact_accepted_source_and_count() {
        for mutation in ["count", "provider", "route"] {
            let temp = tempfile::tempdir().unwrap();
            let entry = UsageCatalogEntry {
                capability: capability(),
                canonical_identity: None,
                provenance_count: 2,
                revision: "revision".to_owned(),
            };
            let catalog = accepted_catalog([entry.clone()]).unwrap();
            let mut projection = empty_projection();
            let previous = projection.clone();
            retain_revoked_accounts(&mut projection, &previous, &catalog, None).unwrap();
            let source = &mut projection.unresolved[0];
            match mutation {
                "count" => source.configuration_count = 99,
                "provider" => source.provider_id = "openai".to_owned(),
                "route" => source.capability_id = "outside-catalog".to_owned(),
                _ => unreachable!(),
            }
            let coordinator = Arc::new(UsageCoordinator::with_catalog(
                Arc::new(ImmediateExecutor),
                Arc::new(MemoryStore::default()),
                UsageCoordinatorConfig::default(),
                [entry.clone()],
            ));
            assert!(
                ProjectionPublisher::new(
                    coordinator,
                    Arc::new(Mutex::new(projection)),
                    FileProjectionStateStore::under_data_dir(temp.path())
                )
                .with_catalog([entry])
                .is_err()
            );
        }
    }

    #[test]
    fn publication_exhaustion_rejects_catalog_rotation_before_any_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let entry = test_entry(capability(), "credential-old".to_owned());
        let executor = Arc::new(ExhaustionExecutor::default());
        let coordinator = Arc::new(UsageCoordinator::with_catalog(
            executor.clone(),
            Arc::new(MemoryStore::default()),
            UsageCoordinatorConfig::default(),
            [entry.clone()],
        ));
        let projection = Arc::new(Mutex::new(empty_projection()));
        let store = FileProjectionStateStore::under_data_dir(temp.path());
        let publisher =
            ProjectionPublisher::new(coordinator.clone(), projection.clone(), store.clone())
                .with_catalog([entry.clone()])
                .unwrap();
        let generation = coordinator
            .request_refresh(&entry.capability, 0, true, 1_000)
            .unwrap()
            .generation;
        coordinator
            .join_generation(&entry.capability, generation, Duration::from_secs(1), 1_001)
            .unwrap();
        publisher.observe(&entry.capability);
        assert!(publisher.publish_due(1_001));
        let before_state = coordinator.current(&entry.capability, 1_001).unwrap();
        {
            let mut current = projection.lock().unwrap();
            current.broker_generation = u64::MAX;
            current.projection_id = format!("test:{}", u64::MAX);
        }
        let before = projection.lock().unwrap().clone();
        let mut envelope = store.load().unwrap().unwrap();
        envelope.projection = before.clone();
        store.store(&envelope).unwrap();
        let error = publisher
            .reconcile_catalog(
                "same-config".to_owned(),
                vec![UsageCatalogEntry {
                    revision: "credential-new".to_owned(),
                    ..entry
                }],
                1_002,
            )
            .unwrap_err();
        assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
        assert_eq!(error.message, "usage publication generation exhausted");
        assert_eq!(*projection.lock().unwrap(), before);
        assert_eq!(store.load().unwrap().unwrap(), envelope);
        assert_eq!(
            coordinator.current(&capability(), 1_002).unwrap(),
            before_state
        );
        assert_eq!(executor.reconciles.load(Ordering::SeqCst), 0);
        assert_eq!(executor.probes.load(Ordering::SeqCst), 1);
        assert!(publisher.current_projection().is_err());
        assert!(!publisher.publish_due(1_002));
    }

    #[test]
    fn exhausted_publication_rejects_manual_refresh_before_provider_dispatch() {
        use jackin_protocol::usage_broker::{
            USAGE_BROKER_PROTOCOL_VERSION, UsageBrokerOperation, UsageBrokerRequest,
            UsageBrokerResponse,
        };
        for ordinal in [u64::MAX - 1, u64::MAX] {
            let temp = tempfile::tempdir().unwrap();
            let executor = Arc::new(ExhaustionExecutor::default());
            let entry = test_entry(capability(), "credential".to_owned());
            let coordinator = Arc::new(UsageCoordinator::with_catalog(
                executor.clone(),
                Arc::new(MemoryStore::default()),
                UsageCoordinatorConfig::default(),
                [entry.clone()],
            ));
            let projection = Arc::new(Mutex::new(empty_projection()));
            let publisher = ProjectionPublisher::new(
                coordinator.clone(),
                projection.clone(),
                FileProjectionStateStore::under_data_dir(temp.path()),
            )
            .with_catalog([entry])
            .unwrap();
            publisher.observe(&capability());
            projection.lock().unwrap().broker_generation = ordinal;
            for operation in [
                UsageBrokerOperation::Refresh {
                    capability: capability(),
                    observed_generation: 0,
                    force: true,
                },
                UsageBrokerOperation::RequestRefresh {
                    force: true,
                    observed_projection_id: None,
                },
                UsageBrokerOperation::CurrentProjection,
            ] {
                let response = super::super::dispatch(
                    &coordinator,
                    UsageBrokerRequest {
                        protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
                        build_id: "fixture-build".to_owned(),
                        operation,
                        launch_credential_scope: None,
                    },
                    "fixture-build",
                    &publisher,
                );
                assert!(matches!(response, UsageBrokerResponse::Error { error }
                    if error.kind == UsageCoordinationErrorKind::Unavailable
                        && error.message == "usage publication generation exhausted"));
            }
            assert_eq!(executor.probes.load(Ordering::SeqCst), 0);
            assert_eq!(
                coordinator
                    .current(&capability(), 1_000)
                    .unwrap()
                    .generation,
                0
            );
            assert!(!publisher.publish_due(1_000));
            assert_eq!(projection.lock().unwrap().broker_generation, ordinal);
        }
        assert_eq!(
            next_publication_generation(u64::MAX - 2).unwrap(),
            u64::MAX - 1
        );
    }
}
