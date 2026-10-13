// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Incremental per-account publication of the canonical broker projection.
//!
//! The broker publishes one immutable [`UsageProjectionV1`] per change. Each
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
//! - A publication that fails [`UsageProjectionV1::validate`] is discarded and
//!   the last-good publication is kept.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, StatusSlot, UsageSeverity, UsageSnapshotStatus,
};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageAccountV1, UsageCatalogEntry, UsageCoordinationError,
    UsageCoordinationErrorKind, UsageFreshnessPhaseV1, UsageFreshnessV1, UsageGenerationView,
    UsageIdentityKindV1, UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1,
    UsageLifecycleV1, UsageLimitWindowV1, UsageMembershipStateV1, UsagePercent,
    UsageProjectionRefreshStateV1, UsageProjectionV1, UsageProviderV1, UsageQuotaStateV1,
    UsageRefreshPhase, UsageWindowCategoryV1,
};

use crate::coordinator::{
    FileProjectionStateStore, ProjectionStateEnvelope, StateStoreError, UsageCoordinator,
};

use super::super::projection::{failure_lifecycle, lifecycle, metric_groups_for_view};

/// Server-side incremental publisher. Cheap to clone; all state is shared.
#[derive(Debug, Clone)]
pub(crate) struct ProjectionPublisher {
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
struct PublishedAccount {
    generation: u64,
    phase: UsageRefreshPhase,
    has_snapshot: bool,
}

/// Canonical identity evidence captured by host discovery and carried into
/// the Capsule-facing projection. A display label is not identity evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AccountIdentityMetadata {
    pub identity_kind: UsageIdentityKindV1,
    pub provenance_count: u32,
}

impl ProjectionPublisher {
    /// Attach a publisher to one broker-owned coordinator and projection.
    pub(crate) fn new(
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
    pub(crate) fn with_catalog(self, entries: impl IntoIterator<Item = UsageCatalogEntry>) -> Self {
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
    pub(crate) fn with_identity_metadata(
        mut self,
        identity_metadata: BTreeMap<UsageAccountCapability, AccountIdentityMetadata>,
    ) -> Self {
        self.identity_metadata = identity_metadata;
        self
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
                .is_none_or(|catalog| catalog.contains_key(capability))
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
    ) -> Result<UsageProjectionV1, UsageCoordinationError> {
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
            schema_version: 2,
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
    pub(crate) fn current_projection(&self) -> Result<UsageProjectionV1, UsageCoordinationError> {
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
    pub(crate) fn publish_due(&self, now_epoch: i64) -> bool {
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
            schema_version: 2,
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

fn retain_revoked_accounts(
    projection: &mut UsageProjectionV1,
    previous: &UsageProjectionV1,
    catalog: &BTreeMap<UsageAccountCapability, String>,
    previous_catalog: Option<&BTreeMap<UsageAccountCapability, String>>,
) {
    for provider in &mut projection.providers {
        provider.accounts.retain(|account| {
            let capability = UsageAccountCapability {
                account_id: account.canonical_account_id.clone(),
                surface_id: provider.provider_id.clone(),
            };
            let revision_changed = previous_catalog.is_some_and(|previous_catalog| {
                previous_catalog
                    .get(&capability)
                    .zip(catalog.get(&capability))
                    .is_some_and(|(previous, current)| previous != current)
            });
            !(catalog.contains_key(&capability)
                && !revision_changed
                && is_revoked_tombstone(account))
        });
        for account in &mut provider.accounts {
            let capability = UsageAccountCapability {
                account_id: account.canonical_account_id.clone(),
                surface_id: provider.provider_id.clone(),
            };
            let revision_changed = previous_catalog.is_some_and(|previous_catalog| {
                previous_catalog
                    .get(&capability)
                    .zip(catalog.get(&capability))
                    .is_some_and(|(previous, current)| previous != current)
            });
            if !catalog.contains_key(&capability) || revision_changed {
                mark_revoked(account);
            }
        }
    }
    projection
        .providers
        .retain(|provider| !provider.accounts.is_empty());

    for previous_provider in &previous.providers {
        for previous_account in &previous_provider.accounts {
            let capability = UsageAccountCapability {
                account_id: previous_account.canonical_account_id.clone(),
                surface_id: previous_provider.provider_id.clone(),
            };
            if catalog.contains_key(&capability)
                || projection.providers.iter().any(|provider| {
                    provider.provider_id == capability.surface_id
                        && provider
                            .accounts
                            .iter()
                            .any(|account| account.canonical_account_id == capability.account_id)
                })
            {
                continue;
            }
            let mut account = previous_account.clone();
            mark_revoked(&mut account);
            if let Some(provider) = projection
                .providers
                .iter_mut()
                .find(|provider| provider.provider_id == capability.surface_id)
            {
                provider.accounts.push(account);
            } else {
                let mut provider = previous_provider.clone();
                provider.accounts = vec![account];
                projection.providers.push(provider);
            }
        }
    }

    projection
        .providers
        .sort_by(|left, right| left.provider_id.cmp(&right.provider_id));
    for (provider_rank, provider) in projection.providers.iter_mut().enumerate() {
        provider.rank = u32::try_from(provider_rank).unwrap_or(u32::MAX);
        provider
            .accounts
            .sort_by(|left, right| left.canonical_account_id.cmp(&right.canonical_account_id));
        for (account_rank, account) in provider.accounts.iter_mut().enumerate() {
            account.rank = u32::try_from(account_rank).unwrap_or(u32::MAX);
        }
    }
}

fn mark_revoked(account: &mut UsageAccountV1) {
    account.status_label = Some("removed".to_owned());
    account.lifecycle = UsageLifecycleV1::Unavailable;
    account.freshness.phase = UsageFreshnessPhaseV1::Failed;
    account.freshness.is_stale = true;
    account.windows.clear();
    account.metric_groups.clear();
    account.issues.clear();
}

fn is_revoked_tombstone(account: &UsageAccountV1) -> bool {
    account.status_label.as_deref() == Some("removed")
        && account.lifecycle == UsageLifecycleV1::Unavailable
        && account.freshness.phase == UsageFreshnessPhaseV1::Failed
        && account.freshness.is_stale
}

fn catalog_entries(catalog: &BTreeMap<UsageAccountCapability, String>) -> Vec<UsageCatalogEntry> {
    catalog
        .iter()
        .map(|(capability, revision)| UsageCatalogEntry {
            capability: capability.clone(),
            revision: revision.clone(),
        })
        .collect()
}

/// Rebuild provider/account rows from per-account generation views.
///
/// Providers and accounts are rebuilt in settled `(surface_id, account_id)`
/// order with canonical ranks. Projection-level `unresolved`, `issues`, and
/// the catalog revision are preserved untouched.
fn merge_views(
    projection: &mut UsageProjectionV1,
    views: &[UsageGenerationView],
    identity_metadata: &BTreeMap<UsageAccountCapability, AccountIdentityMetadata>,
) {
    let mut ordered = views.to_vec();
    ordered.sort_by(|left, right| {
        (&left.capability.surface_id, &left.capability.account_id)
            .cmp(&(&right.capability.surface_id, &right.capability.account_id))
    });
    let any_active = ordered.iter().any(|view| view.phase.is_active());
    projection.refresh_state = if any_active {
        UsageProjectionRefreshStateV1::Refreshing
    } else {
        UsageProjectionRefreshStateV1::Idle
    };
    let mut providers: Vec<UsageProviderV1> = Vec::new();
    for view in &ordered {
        let surface_id = view.capability.surface_id.clone();
        if providers
            .last()
            .is_none_or(|provider: &UsageProviderV1| provider.provider_id != surface_id)
        {
            providers.push(UsageProviderV1 {
                provider_id: surface_id.clone(),
                display_name: surface_id.clone(),
                rank: u32::try_from(providers.len()).unwrap_or(u32::MAX),
                membership_state: UsageMembershipStateV1::Current,
                freshness: UsageFreshnessV1 {
                    generation: 0,
                    phase: UsageFreshnessPhaseV1::Failed,
                    last_good_at_epoch: None,
                    retry_at_epoch: None,
                    is_stale: false,
                },
                accounts: Vec::new(),
                issues: Vec::new(),
            });
        }
        let Some(provider) = providers.last_mut() else {
            continue;
        };
        let account = account_for_view(
            view,
            provider.accounts.len(),
            identity_metadata.get(&view.capability),
        );
        if let Some(snapshot) = &view.snapshot
            && !snapshot.account.provider_label.is_empty()
        {
            provider.display_name = snapshot.account.provider_label.clone();
        }
        provider.accounts.push(account);
    }
    for provider in &mut providers {
        let provider_active = ordered
            .iter()
            .filter(|view| view.capability.surface_id == provider.provider_id)
            .any(|view| view.phase.is_active());
        provider.freshness = aggregate_freshness(provider_active, &provider.accounts);
    }
    projection.providers = providers;
}

fn aggregate_freshness(any_active: bool, accounts: &[UsageAccountV1]) -> UsageFreshnessV1 {
    let mut freshness = UsageFreshnessV1 {
        generation: 0,
        phase: UsageFreshnessPhaseV1::Failed,
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
        UsageFreshnessPhaseV1::Refreshing
    } else if accounts
        .iter()
        .all(|account| account.freshness.phase == UsageFreshnessPhaseV1::Failed)
    {
        UsageFreshnessPhaseV1::Failed
    } else if accounts
        .iter()
        .any(|account| account.freshness.phase == UsageFreshnessPhaseV1::Stale)
    {
        UsageFreshnessPhaseV1::Stale
    } else {
        UsageFreshnessPhaseV1::Current
    };
    freshness
}

fn account_for_view(
    view: &UsageGenerationView,
    rank: usize,
    identity_metadata: Option<&AccountIdentityMetadata>,
) -> UsageAccountV1 {
    let snapshot = view.snapshot.clone();
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
                .map_or(UsageLifecycleV1::Unavailable, |error| {
                    failure_lifecycle(error.kind)
                })
        },
        |snapshot| lifecycle(snapshot.status, snapshot.confidence),
    );
    let is_stale = snapshot.as_ref().is_some_and(|snapshot| {
        matches!(snapshot.status, UsageSnapshotStatus::Stale)
            || view.phase == UsageRefreshPhase::Failed
    });
    let phase = if view.phase.is_active() {
        UsageFreshnessPhaseV1::Refreshing
    } else if view.snapshot.is_none() {
        UsageFreshnessPhaseV1::Failed
    } else if is_stale {
        UsageFreshnessPhaseV1::Stale
    } else {
        UsageFreshnessPhaseV1::Current
    };
    let windows = snapshot
        .as_ref()
        .map(|snapshot| windows_for_snapshot(&view.capability.account_id, snapshot))
        .unwrap_or_default();
    let metric_groups = snapshot
        .as_ref()
        .and_then(|snapshot| {
            metric_groups_for_view(
                &view.capability.account_id,
                snapshot,
                snapshot.account.plan_label.as_deref(),
            )
            .ok()
        })
        .unwrap_or_default();
    let issues = view
        .error
        .as_ref()
        .map(|error| {
            vec![UsageIssueV1 {
                code: issue_code(error.kind),
                scope: UsageIssueScopeV1::Account,
                recoverability: issue_recoverability(error.kind),
                message: error.message.clone(),
                retry_at_epoch: view.retry_at_epoch,
            }]
        })
        .unwrap_or_default();
    let fallback_identity_kind = if header_label.trim().is_empty() {
        UsageIdentityKindV1::ProviderAccountId
    } else {
        UsageIdentityKindV1::ProviderStableHandle
    };
    UsageAccountV1 {
        canonical_account_id: view.capability.account_id.clone(),
        identity_kind: identity_metadata
            .map_or(fallback_identity_kind, |metadata| metadata.identity_kind),
        rank: u32::try_from(rank).unwrap_or(u32::MAX),
        display_label,
        plan_label: snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.account.plan_label.clone()),
        status_label: None,
        lifecycle,
        freshness: UsageFreshnessV1 {
            generation: view.generation,
            phase,
            last_good_at_epoch: snapshot.as_ref().map(|snapshot| snapshot.fetched_at_epoch),
            retry_at_epoch: view.retry_at_epoch,
            is_stale,
        },
        provenance_count: identity_metadata.map_or(1, |metadata| metadata.provenance_count),
        windows,
        metric_groups,
        credential_expires_at_epoch: None,
        issues,
    }
}

fn windows_for_snapshot(account_id: &str, snapshot: &FocusedUsageView) -> Vec<UsageLimitWindowV1> {
    snapshot
        .buckets
        .iter()
        .enumerate()
        .map(|(rank, bucket)| window_for_bucket(account_id, rank, bucket))
        .collect()
}

fn window_for_bucket(
    account_id: &str,
    rank: usize,
    bucket: &QuotaBucketView,
) -> UsageLimitWindowV1 {
    let category = match bucket.status_slot {
        Some(StatusSlot::Session) => UsageWindowCategoryV1::Session,
        Some(StatusSlot::Daily | StatusSlot::Weekly) => UsageWindowCategoryV1::LongRange,
        Some(StatusSlot::Spend) | None => UsageWindowCategoryV1::Other,
    };
    let raw_used = money_used_raw_percent(bucket);
    let overage = raw_used.is_some_and(|value| value > 100);
    let (remaining_percent, remaining_raw_percent) = if overage {
        (None, None)
    } else {
        bucket.remaining_percent.map_or((None, None), |percent| {
            let clamped = UsagePercent::clamp_raw(i32::from(percent));
            (Some(clamped), Some(i32::from(percent)))
        })
    };
    let (used_percent, used_raw_percent) = if overage || remaining_percent.is_none() {
        raw_used.map_or((None, None), |raw| {
            (Some(UsagePercent::clamp_raw(raw)), Some(raw))
        })
    } else {
        (None, None)
    };
    let value_label = if overage {
        raw_used.map_or_else(
            || bucket.used_label.clone().unwrap_or_default(),
            |raw| format!("{raw}% used"),
        )
    } else {
        match (&bucket.used_label, &bucket.limit_label) {
            (Some(used), Some(limit)) => format!("{used} of {limit}"),
            (Some(used), None) => used.clone(),
            (None, Some(limit)) => limit.clone(),
            (None, None) => bucket
                .remaining_percent
                .map_or_else(String::new, |percent| format!("{percent}% left")),
        }
    };
    UsageLimitWindowV1 {
        window_id: format!("{account_id}:{rank}"),
        rank: u32::try_from(rank).unwrap_or(u32::MAX),
        category,
        label: bucket.label.clone(),
        value_label,
        reset_label: bucket.reset_label.clone().unwrap_or_default(),
        remaining_percent,
        remaining_raw_percent,
        used_percent,
        used_raw_percent,
        reset_at_epoch: bucket.resets_at,
        quota_state: quota_state_for_bucket(bucket),
        pace_label: bucket.pace_label.clone(),
        runs_out_label: None,
    }
}

/// Raw used percentage for money-backed quota windows. The broker publisher
/// must preserve overage just like the desktop projection; only bar geometry
/// is clamped.
fn money_used_raw_percent(bucket: &QuotaBucketView) -> Option<i32> {
    let used = bucket.used_money.as_ref()?;
    let limit = bucket.limit_money.as_ref()?;
    if used.currency != limit.currency || used.exponent != limit.exponent || limit.amount_minor <= 0
    {
        return None;
    }
    let scaled = used.amount_minor.saturating_mul(100);
    let raw = scaled.checked_div(limit.amount_minor)?;
    Some(i32::try_from(raw).unwrap_or(if raw < 0 { i32::MIN } else { i32::MAX }))
}

fn quota_state_for_bucket(bucket: &QuotaBucketView) -> UsageQuotaStateV1 {
    match bucket.status {
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => {
            if bucket.remaining_percent == Some(0) || money_is_exhausted(bucket) {
                UsageQuotaStateV1::Exhausted
            } else {
                match bucket.severity {
                    UsageSeverity::Danger => UsageQuotaStateV1::Exhausted,
                    UsageSeverity::Warn => UsageQuotaStateV1::Warning,
                    UsageSeverity::Normal => {
                        if bucket_has_quantity(bucket) {
                            UsageQuotaStateV1::Available
                        } else {
                            UsageQuotaStateV1::Unknown
                        }
                    }
                }
            }
        }
        UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::NeedsSecret => {
            UsageQuotaStateV1::NoPermission
        }
        UsageSnapshotStatus::Unsupported => UsageQuotaStateV1::Unsupported,
        UsageSnapshotStatus::Unavailable => UsageQuotaStateV1::Unavailable,
        UsageSnapshotStatus::Error => UsageQuotaStateV1::Error,
    }
}

fn money_is_exhausted(bucket: &QuotaBucketView) -> bool {
    match (bucket.used_money.as_ref(), bucket.limit_money.as_ref()) {
        (Some(used), Some(limit)) => {
            used.currency == limit.currency
                && used.exponent == limit.exponent
                && limit.amount_minor > 0
                && used.amount_minor >= limit.amount_minor
        }
        _ => false,
    }
}

fn bucket_has_quantity(bucket: &QuotaBucketView) -> bool {
    bucket.remaining_percent.is_some()
        || bucket.used_money.is_some()
        || bucket.limit_money.is_some()
        || bucket.used_label.is_some()
        || bucket.limit_label.is_some()
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
) -> UsageIssueRecoverabilityV1 {
    match kind {
        UsageCoordinationErrorKind::NeedsSecret | UsageCoordinationErrorKind::Unauthorized => {
            UsageIssueRecoverabilityV1::ActionRequired
        }
        UsageCoordinationErrorKind::ProtocolMismatch
        | UsageCoordinationErrorKind::CorruptState
        | UsageCoordinationErrorKind::OwnerLost
        | UsageCoordinationErrorKind::CatalogRevoked
        | UsageCoordinationErrorKind::CatalogRevisionConflict => {
            UsageIssueRecoverabilityV1::Terminal
        }
        _ => UsageIssueRecoverabilityV1::Retryable,
    }
}

#[cfg(test)]
mod tests;
