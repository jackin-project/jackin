// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
        .reconcile_catalog("catalog-2".to_owned(), Vec::new(), 1_002)
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
    let third = UsageAccountCapability {
        account_id: "account-c".to_owned(),
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
        UsageGenerationView {
            capability: third.clone(),
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
        (
            third,
            AccountIdentityMetadata {
                identity_kind: UsageIdentityKindV1::LocalSourceHandle,
                provenance_count: 1,
            },
        ),
    ]);
    let mut projection = empty_projection();

    merge_views(&mut projection, &views, &metadata);

    let accounts = &projection.providers[0].accounts;
    assert_eq!(accounts.len(), 3);
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
    assert_eq!(
        accounts[2].identity_kind,
        UsageIdentityKindV1::LocalSourceHandle
    );
    assert_eq!(accounts[2].provenance_count, 1);
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
