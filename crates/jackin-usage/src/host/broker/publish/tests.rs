// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_protocol::control::{Money, UsageConfidence, UsageSeverity, UsageSource};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageFreshnessPhaseV1, UsageIdentityKindV1, UsageLifecycleV1,
    UsageMetricValueV1, UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageQuotaStateV1,
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

fn empty_projection() -> UsageProjectionV1 {
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "test:0".to_owned(),
        generated_at_epoch: 1_000,
        discovery_revision: "catalog".to_owned(),
        broker_instance_id: "test".to_owned(),
        broker_generation: 0,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

#[test]
fn catalog_helpers_read_only_the_attached_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let account = capability();
    let catalog_entry = UsageCatalogEntry {
        capability: account.clone(),
        revision: "credential-a".to_owned(),
    };
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        Arc::new(ImmediateExecutor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
        [catalog_entry.clone()],
    ));
    let publisher = ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::new(Mutex::new(empty_projection())),
        FileProjectionStateStore::under_data_dir(temp.path()),
    );

    assert!(!publisher.is_catalog_member(&account));
    assert!(publisher.catalog_capabilities().is_empty());

    let publisher = publisher.with_catalog([catalog_entry]);
    assert!(publisher.is_catalog_member(&account));
    assert_eq!(publisher.catalog_capabilities(), vec![account.clone()]);
    assert!(!publisher.is_catalog_member(&UsageAccountCapability {
        account_id: "other-account".to_owned(),
        surface_id: account.surface_id,
    }));
}

#[test]
fn broker_conflict_is_unavailable_and_keeps_its_issue_code() {
    let account = account_for_view(
        &UsageGenerationView {
            capability: capability(),
            generation: 1,
            phase: UsageRefreshPhase::Failed,
            snapshot: None,
            error: Some(UsageCoordinationError {
                kind: UsageCoordinationErrorKind::BrokerConflict,
                message: "another broker owns the lease".to_owned(),
            }),
            retry_at_epoch: None,
        },
        0,
        None,
    );

    assert_eq!(account.lifecycle, UsageLifecycleV1::Unavailable);
    assert_eq!(account.issues[0].code, "broker_conflict");
    assert_eq!(
        account.issues[0].recoverability,
        UsageIssueRecoverabilityV1::Terminal
    );
}

#[test]
fn catalog_reconciliation_publishes_removed_tombstone_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let account = capability();
    let catalog_entry = UsageCatalogEntry {
        capability: account.clone(),
        revision: "credential-a".to_owned(),
    };
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
    .with_catalog([catalog_entry.clone()]);

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
        .reconcile_catalog_if_projection_with_diagnostics(
            None,
            "catalog-2".to_owned(),
            Vec::new(),
            CatalogDiagnostics::default(),
            1_002,
        )
        .unwrap();
    let account_row = &removed.providers[0].accounts[0];
    assert_eq!(account_row.canonical_account_id, account.account_id);
    assert_eq!(account_row.display_label, "account@example.test");
    assert_eq!(account_row.status_label.as_deref(), Some("removed"));
    assert_eq!(account_row.lifecycle, UsageLifecycleV1::Unavailable);
    assert_eq!(account_row.freshness.phase, UsageFreshnessPhaseV1::Failed);
    assert_eq!(removed.discovery_revision, "catalog-2");

    let persisted = store.load().unwrap().unwrap();
    assert!(persisted.catalog.is_empty());
    assert_eq!(persisted.projection, removed);
}

fn account_with_retry(id: &str, retry_at_epoch: Option<i64>) -> UsageAccountV1 {
    UsageAccountV1 {
        canonical_account_id: id.to_owned(),
        identity_kind: UsageIdentityKindV1::ProviderAccountId,
        rank: 0,
        display_label: id.to_owned(),
        plan_label: None,
        status_label: None,
        lifecycle: UsageLifecycleV1::Available,
        freshness: UsageFreshnessV1 {
            generation: 1,
            phase: UsageFreshnessPhaseV1::Failed,
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
    let metadata = BTreeMap::from([
        (
            first,
            AccountIdentityMetadata {
                identity_kind: UsageIdentityKindV1::ProviderAccountId,
                provenance_count: 3,
            },
        ),
        (
            second,
            AccountIdentityMetadata {
                identity_kind: UsageIdentityKindV1::ProviderStableHandle,
                provenance_count: 2,
            },
        ),
    ]);
    let mut projection = empty_projection();

    merge_views(&mut projection, &views, &metadata);

    let accounts = &projection.providers[0].accounts;
    assert_eq!(accounts.len(), 2);
    assert_eq!(
        accounts[0].identity_kind,
        UsageIdentityKindV1::ProviderAccountId
    );
    assert_eq!(accounts[0].provenance_count, 3);
    assert_eq!(
        accounts[1].identity_kind,
        UsageIdentityKindV1::ProviderStableHandle
    );
    assert_eq!(accounts[1].provenance_count, 2);
}

#[test]
fn capsule_publication_preserves_openrouter_overage_raw_used_percent() {
    let capability = capability();
    let mut view = fresh_view();
    view.buckets = vec![QuotaBucketView {
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

    merge_views(&mut projection, &views, &BTreeMap::new());

    let window = &projection.providers[0].accounts[0].windows[0];
    assert_eq!(window.value_label, "120% used");
    assert_eq!(window.used_percent.map(UsagePercent::get), Some(100));
    assert_eq!(window.used_raw_percent, Some(120));
    assert_eq!(window.remaining_percent, None);
    assert_eq!(window.remaining_raw_percent, None);
    assert_eq!(window.quota_state, UsageQuotaStateV1::Exhausted);
    match &projection.providers[0].accounts[0].metric_groups[1].value {
        UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            assert_eq!(cap, &Some(Money::new(10_000, "USD", 2)));
            assert_eq!(spent, &Some(Money::new(12_000, "USD", 2)));
            assert_eq!(remaining, &Some(Money::new(0, "USD", 2)));
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
        merge_views(&mut projection, &views, &BTreeMap::new());
        projection
    };
    assert_eq!(
        unknown_projection.providers[0].accounts[0].windows[0].quota_state,
        UsageQuotaStateV1::Unknown
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
    merge_views(&mut projection, &views, &BTreeMap::new());
    let account = &projection.providers[0].accounts[0];
    assert_eq!(account.freshness.phase, UsageFreshnessPhaseV1::Stale);
    assert!(account.freshness.is_stale);
    assert_eq!(account.windows[0].quota_state, UsageQuotaStateV1::Available);
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
    merge_views(&mut projection, &views, &BTreeMap::new());

    assert_eq!(
        projection.refresh_state,
        UsageProjectionRefreshStateV1::Refreshing
    );
    assert_eq!(
        projection
            .providers
            .iter()
            .find(|provider| provider.provider_id == "claude")
            .map(|provider| provider.freshness.phase),
        Some(UsageFreshnessPhaseV1::Refreshing)
    );
    assert_eq!(
        projection
            .providers
            .iter()
            .find(|provider| provider.provider_id == "codex")
            .map(|provider| provider.freshness.phase),
        Some(UsageFreshnessPhaseV1::Current)
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
    let coordinator = Arc::new(UsageCoordinator::new(
        Arc::new(ImmediateExecutor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
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
    );
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
fn catalog_publication_retains_removed_rows_without_expanding_to_new_members() {
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
            UsageCatalogEntry {
                capability: account_a.clone(),
                revision: "revision-a".to_owned(),
            },
            UsageCatalogEntry {
                capability: account_b.clone(),
                revision: "revision-b".to_owned(),
            },
        ],
    ));
    let projection = Arc::new(Mutex::new(empty_projection()));
    let publisher = ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        FileProjectionStateStore::under_data_dir(temp.path()),
    )
    .with_catalog([
        UsageCatalogEntry {
            capability: account_a.clone(),
            revision: "revision-a".to_owned(),
        },
        UsageCatalogEntry {
            capability: account_b.clone(),
            revision: "revision-b".to_owned(),
        },
    ]);

    let queued = coordinator
        .request_refresh(&account_a, 0, true, 1_000)
        .unwrap();
    coordinator
        .join_generation(&account_a, queued.generation, Duration::from_secs(2), 1_001)
        .unwrap();
    publisher.observe(&account_a);
    assert!(publisher.publish_due(1_002));
    assert_eq!(projection.lock().unwrap().providers[0].accounts.len(), 1);

    let current = publisher
        .reconcile_catalog_if_projection_with_diagnostics(
            None,
            "catalog-2".to_owned(),
            vec![UsageCatalogEntry {
                capability: account_b.clone(),
                revision: "revision-b".to_owned(),
            }],
            CatalogDiagnostics::default(),
            1_003,
        )
        .unwrap();
    let removed = current
        .providers
        .iter()
        .flat_map(|provider| provider.accounts.iter())
        .find(|account| account.canonical_account_id == account_a.account_id)
        .expect("removed account remains visible as a tombstone");
    assert_eq!(removed.status_label.as_deref(), Some("removed"));
    assert_eq!(removed.lifecycle, UsageLifecycleV1::Unavailable);
    assert!(publisher.known_capabilities().is_empty());
    let persisted = FileProjectionStateStore::under_data_dir(temp.path())
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(persisted.catalog.len(), 1);
    assert_eq!(persisted.catalog[0].capability, account_b);

    let reintroduced = publisher
        .reconcile_catalog_if_projection_with_diagnostics(
            None,
            "catalog-3".to_owned(),
            vec![UsageCatalogEntry {
                capability: account_a,
                revision: "revision-a".to_owned(),
            }],
            CatalogDiagnostics::default(),
            1_004,
        )
        .unwrap();
    assert!(reintroduced.providers.is_empty());
}

#[test]
fn same_capability_revision_purges_stale_published_quota() {
    let temp = tempfile::tempdir().unwrap();
    let account = capability();
    let old = UsageCatalogEntry {
        capability: account.clone(),
        revision: "credential-a".to_owned(),
    };
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
    .with_catalog([old]);

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
        .reconcile_catalog_if_projection_with_diagnostics(
            None,
            "catalog".to_owned(),
            vec![UsageCatalogEntry {
                capability: account.clone(),
                revision: "credential-b".to_owned(),
            }],
            CatalogDiagnostics::default(),
            1_002,
        )
        .unwrap();
    let row = &current.providers[0].accounts[0];
    assert_eq!(row.status_label.as_deref(), Some("removed"));
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
    let old = UsageCatalogEntry {
        capability: account.clone(),
        revision: "credential-a".to_owned(),
    };
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
    .with_catalog([old.clone()]);

    let error = publisher
        .reconcile_catalog_if_projection_with_diagnostics(
            None,
            "new-catalog".to_owned(),
            Vec::new(),
            CatalogDiagnostics::default(),
            1_001,
        )
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
    let old = UsageCatalogEntry {
        capability: account.clone(),
        revision: "credential-a".to_owned(),
    };
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
    .with_catalog([old]);

    let error = publisher
        .reconcile_catalog_if_projection_with_diagnostics(
            None,
            "new-catalog".to_owned(),
            Vec::new(),
            CatalogDiagnostics::default(),
            1_001,
        )
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
            UsageLifecycleV1::NeedsLogin,
        ),
        (
            UsageSnapshotStatus::NeedsSecret,
            UsageLifecycleV1::NeedsSecret,
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
            None,
        );
        assert_eq!(account.lifecycle, expected);
    }
    for (kind, expected) in [
        (
            UsageCoordinationErrorKind::Unauthorized,
            UsageLifecycleV1::NeedsLogin,
        ),
        (
            UsageCoordinationErrorKind::NeedsSecret,
            UsageLifecycleV1::NeedsSecret,
        ),
        (
            UsageCoordinationErrorKind::ProtocolMismatch,
            UsageLifecycleV1::Unsupported,
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
            None,
        );
        assert_eq!(account.lifecycle, expected);
    }
}
