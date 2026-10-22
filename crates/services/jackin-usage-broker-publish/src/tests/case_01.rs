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

    merge_views(&mut projection, &views, &metadata).expect("valid views project");

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
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota: None,
        provider_quota_state: None,
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

    merge_views(&mut projection, &views, &BTreeMap::new()).expect("valid views project");

    let window = &projection.providers[0].accounts[0].windows[0];
    assert_eq!(window.value_label, "120% used");
    assert_eq!(window.used_percent, None);
    assert_eq!(
        window.money_used_percent.map(RawPercentPoints::get),
        Some(120.0)
    );
    assert_eq!(
        window
            .meter_percent
            .map(jackin_protocol::control::MeterPercentPoints::get),
        Some(100.0)
    );
    assert_eq!(window.meter_side, Some(UsageMeterSide::Used));
    assert_eq!(window.remaining_percent, None);
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
fn window_and_group_keep_fractional_sources_count_geometry_and_money_ratio() {
    let mut count_bucket = fresh_view().buckets.remove(0);
    count_bucket.remaining_percent =
        Some(CanonicalRemainingPercent::new(57.125).expect("valid raw remaining source"));
    count_bucket.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    count_bucket.count_quota = Some(CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    let window = project_window("canon-count", &count_bucket, 0).expect("valid count window");
    assert_eq!(
        window.remaining_percent.map(RawPercentPoints::get),
        Some(57.125)
    );
    assert_eq!(
        window.remaining_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        window
            .meter_percent
            .map(jackin_protocol::control::MeterPercentPoints::get),
        Some(10.0)
    );
    assert_eq!(window.meter_side, Some(UsageMeterSide::Remaining));
    assert_eq!(window.count_quota, count_bucket.count_quota);
    let group = project_window_group(
        "canon-count",
        &count_bucket,
        UsageSnapshotStatus::Fresh,
        1_000,
        0,
    )
    .expect("valid count group");
    group.validate(0).expect("group validation");
    assert!(matches!(
        &group.value,
        UsageMetricValueV1::Window {
            remaining_percent: Some(remaining),
            remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
            meter_percent: Some(meter),
            meter_side: Some(UsageMeterSide::Remaining),
            count_quota: Some(count),
            provider_quota_state: None,
            period: jackin_protocol::usage_broker::UsageMetricPeriodV1::Calendar {
                granularity: jackin_protocol::usage_broker::UsageCalendarPeriodV1::Daily,
            },
            ..
        } if remaining.get().partial_cmp(&57.125) == Some(std::cmp::Ordering::Equal)
            && meter.get().partial_cmp(&10.0) == Some(std::cmp::Ordering::Equal)
            && *count == count_bucket.count_quota.unwrap()
    ));

    let mut incomplete = count_bucket;
    incomplete.count_quota = Some(CountQuota::new(
        Some(9),
        None,
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    let incomplete_window =
        project_window("canon-incomplete", &incomplete, 0).expect("valid incomplete count window");
    assert_eq!(
        incomplete_window
            .remaining_percent
            .map(RawPercentPoints::get),
        Some(57.125)
    );
    assert_eq!(incomplete_window.meter_percent, None);
    assert_eq!(incomplete_window.meter_side, None);

    let mut signed_money = fresh_view().buckets.remove(0);
    signed_money.used_percent =
        Some(CanonicalUsedPercent::new(43.375).expect("valid raw used source"));
    signed_money.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    signed_money.remaining_percent =
        Some(CanonicalRemainingPercent::new(12.625).expect("valid raw remaining source"));
    signed_money.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    signed_money.used_money = Some(Money::new(-10, "USD", 2));
    signed_money.limit_money = Some(Money::new(100, "USD", 2));
    let signed_window =
        project_window("canon-money", &signed_money, 0).expect("valid signed money window");
    assert_eq!(
        signed_window.used_percent.map(RawPercentPoints::get),
        Some(43.375)
    );
    assert_eq!(
        signed_window.remaining_percent.map(RawPercentPoints::get),
        Some(12.625)
    );
    assert_eq!(
        signed_window.money_used_percent.map(RawPercentPoints::get),
        Some(-10.0)
    );
    assert_eq!(
        signed_window
            .meter_percent
            .map(jackin_protocol::control::MeterPercentPoints::get),
        Some(43.375)
    );
    assert_eq!(signed_window.meter_side, Some(UsageMeterSide::Used));
    let signed_group = project_window_group(
        "canon-money",
        &signed_money,
        UsageSnapshotStatus::Fresh,
        1_000,
        0,
    )
    .expect("valid signed Money group");
    signed_group.validate(0).expect("signed group validation");
    assert!(matches!(
        &signed_group.value,
        UsageMetricValueV1::Window {
            used_percent: Some(used),
            used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
            remaining_percent: Some(remaining),
            remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
            meter_percent: Some(meter),
            meter_side: Some(UsageMeterSide::Used),
            money_used_percent: Some(money),
            ..
        } if used.get().partial_cmp(&43.375) == Some(std::cmp::Ordering::Equal)
            && remaining.get().partial_cmp(&12.625) == Some(std::cmp::Ordering::Equal)
            && meter.get().partial_cmp(&43.375) == Some(std::cmp::Ordering::Equal)
            && money.get().partial_cmp(&-10.0) == Some(std::cmp::Ordering::Equal)
    ));
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
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota: None,
        provider_quota_state: None,
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
        merge_views(&mut projection, &views, &BTreeMap::new()).expect("valid views project");
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
    merge_views(&mut projection, &views, &BTreeMap::new()).expect("valid views project");
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
    merge_views(&mut projection, &views, &BTreeMap::new()).expect("valid views project");

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
