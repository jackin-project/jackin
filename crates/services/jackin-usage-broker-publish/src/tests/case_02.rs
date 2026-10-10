// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
        .reconcile_catalog(
            "catalog-2".to_owned(),
            vec![UsageCatalogEntry {
                capability: account_b.clone(),
                revision: "revision-b".to_owned(),
            }],
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
        .reconcile_catalog(
            "catalog-3".to_owned(),
            vec![UsageCatalogEntry {
                capability: account_a,
                revision: "revision-a".to_owned(),
            }],
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
        .reconcile_catalog(
            "catalog".to_owned(),
            vec![UsageCatalogEntry {
                capability: account.clone(),
                revision: "credential-b".to_owned(),
            }],
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
        assert_eq!(account.identity_kind, UsageIdentityKindV1::UnverifiedHandle);
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
