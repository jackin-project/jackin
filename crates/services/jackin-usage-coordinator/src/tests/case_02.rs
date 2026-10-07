// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn coordinator_rate_limit_without_provider_deadline_uses_shared_backoff() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::RateLimited,
        message: "provider rate limited".into(),
        retry_at_epoch: None,
    }));
    let store = Arc::new(MemoryStore::default());
    let coordinator = coordinator(
        Arc::<GateExecutor>::clone(&executor),
        Arc::<MemoryStore>::clone(&store),
        UsageCoordinatorConfig::default(),
    );
    let account = capability("account-a");

    coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    executor.release(1);
    let first = join_ok(&coordinator, &account, 1, 1_001);
    assert!(
        first
            .retry_at_epoch
            .is_some_and(|deadline| (1_001..=1_031).contains(&deadline))
    );
    let first_deadline = first.retry_at_epoch.expect("first retry deadline");
    let suppressed = coordinator
        .request_refresh(&account, 1, true, first_deadline.saturating_sub(1))
        .unwrap();
    assert_eq!(suppressed.generation, 1);

    let second_start = first_deadline.saturating_add(1);
    coordinator
        .request_refresh(&account, 1, true, second_start)
        .unwrap();
    executor.wait_started(2);
    executor.release(1);
    let second = join_ok(&coordinator, &account, 2, second_start.saturating_add(1));
    assert!(
        second
            .retry_at_epoch
            .is_some_and(|deadline| (second_start..=second_start + 61).contains(&deadline))
    );
    assert_eq!(
        store
            .states
            .lock()
            .unwrap()
            .get(&account)
            .unwrap()
            .consecutive_failures,
        2
    );
}

#[test]
fn coordinator_timeout_wait_keeps_owner_until_worker_terminates() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let config = UsageCoordinatorConfig {
        provider_timeout: Duration::from_millis(10),
        ..UsageCoordinatorConfig::default()
    };
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        config,
    );
    let account = capability("account-a");
    coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    let error = coordinator
        .join_generation(&account, 1, Duration::from_millis(20), 1_000)
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::WaitTimeout);
    assert_eq!(
        coordinator.current(&account, 1_000).unwrap().phase,
        UsageRefreshPhase::Updating
    );
    executor.release(1);
    let terminal = join_ok(&coordinator, &account, 1, 1_001);
    assert_eq!(terminal.phase, UsageRefreshPhase::Failed);
    assert_eq!(
        terminal.error.unwrap().kind,
        UsageCoordinationErrorKind::ProviderTimeout
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn coordinator_recovers_persisted_owner_loss_once_without_a_herd() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_001, 79),
    )));
    let store = Arc::new(MemoryStore::default());
    let account = capability("account-a");
    let mut abandoned = AccountStateEnvelope::idle(account.clone());
    abandoned.generation = 4;
    abandoned.phase = UsageRefreshPhase::Updating;
    abandoned.started_at_epoch = Some(1_000);
    store
        .states
        .lock()
        .unwrap()
        .insert(account.clone(), abandoned);
    let coordinator = coordinator(
        Arc::<GateExecutor>::clone(&executor),
        Arc::<MemoryStore>::clone(&store),
        UsageCoordinatorConfig::default(),
    );

    let recovered = coordinator
        .request_refresh(&account, 0, true, 1_001)
        .unwrap();
    executor.wait_started(1);
    let joiner = coordinator
        .request_refresh(&account, 0, true, 1_001)
        .unwrap();
    assert_eq!(recovered.generation, 5);
    assert_eq!(joiner.generation, 5);
    assert!(joiner.phase.is_active());

    executor.release(1);
    let terminal = join_ok(&coordinator, &account, 5, 1_002);
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn coordinator_updating_store_failure_terminates_owner() {
    assert_updating_store_failure_terminates(false);
}

#[test]
fn coordinator_updating_store_failure_recovers_after_terminal_store_failure() {
    assert_updating_store_failure_terminates(true);
}

#[test]
fn catalog_revocation_retains_materialized_last_good_but_fences_late_result() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(MemoryStore::default());
    let account = capability("account-a");
    let coordinator = UsageCoordinator::with_catalog(
        Arc::<GateExecutor>::clone(&executor),
        Arc::<MemoryStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "revision-a")],
    );

    let first = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    executor.release(1);
    let first = join_ok(&coordinator, &account, first.generation, 1_001);
    assert_eq!(first.phase, UsageRefreshPhase::Completed);

    let second = coordinator
        .request_refresh(&account, first.generation, true, 1_002)
        .unwrap();
    executor.wait_started(2);
    coordinator
        .reconcile_catalog([], 1_003)
        .expect("catalog removal is durable");
    let revoked = coordinator.current(&account, 1_003).unwrap();
    assert_eq!(revoked.phase, UsageRefreshPhase::Failed);
    assert_eq!(revoked.generation, second.generation + 1);
    assert_eq!(
        revoked.error.as_ref().unwrap().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
    assert_eq!(
        revoked.snapshot.as_ref().unwrap().buckets[0].remaining_percent,
        Some(80)
    );
    assert!(
        coordinator.is_idle(),
        "revocation must clear active ownership"
    );
    assert_eq!(
        coordinator
            .join_generation(
                &account,
                second.generation,
                Duration::from_millis(20),
                1_003,
            )
            .unwrap_err()
            .kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
    assert!(store.purges.lock().unwrap().contains(&account));

    executor.release(1);
    executor.wait_idle();
    let after_late_result = coordinator.current(&account, 1_004).unwrap();
    assert_eq!(
        after_late_result.error.as_ref().unwrap().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
    assert_eq!(after_late_result.generation, revoked.generation);
    assert_eq!(
        after_late_result.snapshot.as_ref().unwrap().buckets[0].remaining_percent,
        Some(80)
    );
    assert_eq!(
        coordinator
            .request_refresh(&account, revoked.generation, true, 1_004)
            .unwrap_err()
            .kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
}

#[test]
fn catalog_revision_change_purges_old_state_and_allows_only_new_revision() {
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(MemoryStore::default());
    let account = capability("account-a");
    let coordinator = UsageCoordinator::with_catalog(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<MemoryStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "revision-a")],
    );
    let first = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    assert_eq!(
        join_ok(&coordinator, &account, first.generation, 1_001).phase,
        UsageRefreshPhase::Completed
    );

    coordinator
        .reconcile_catalog([catalog_entry(&account, "revision-b")], 1_002)
        .unwrap();
    let reset = coordinator.current(&account, 1_002).unwrap();
    assert_eq!(reset.phase, UsageRefreshPhase::Idle);
    assert!(reset.snapshot.is_none());
    assert!(reset.error.is_none());
    assert_eq!(
        store.purges.lock().unwrap().as_slice(),
        std::slice::from_ref(&account)
    );

    let next = coordinator
        .request_refresh(&account, reset.generation, true, 1_003)
        .unwrap();
    assert_eq!(
        join_ok(&coordinator, &account, next.generation, 1_004).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn catalog_purge_failure_restores_durable_state_and_keeps_old_catalog() {
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(MemoryStore::default());
    let account = capability("account-a");
    let coordinator = UsageCoordinator::with_catalog(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<MemoryStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "revision-a")],
    );
    let generation = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap()
        .generation;
    assert_eq!(
        join_ok(&coordinator, &account, generation, 1_001).phase,
        UsageRefreshPhase::Completed
    );
    let before = store.states.lock().unwrap().get(&account).cloned();
    *store.purge_error.lock().unwrap() = Some(StateStoreError::Unavailable);

    let error = coordinator.reconcile_catalog([], 1_002).unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(store.states.lock().unwrap().get(&account).cloned(), before);
    assert_eq!(
        coordinator.current(&account, 1_002).unwrap().phase,
        UsageRefreshPhase::Completed
    );

    *store.purge_error.lock().unwrap() = None;
    coordinator.reconcile_catalog([], 1_003).unwrap();
    assert_eq!(
        coordinator.current(&account, 1_003).unwrap().phase,
        UsageRefreshPhase::Failed
    );
}

#[test]
fn same_capability_revision_change_fences_in_flight_join_immediately() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        Arc::<GateExecutor>::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
        [catalog_entry(&capability("account-a"), "revision-a")],
    ));
    let account = capability("account-a");
    let generation = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap()
        .generation;
    executor.wait_started(1);

    let join_coordinator = Arc::clone(&coordinator);
    let join_account = account.clone();
    let started = Instant::now();
    let joiner = std::thread::spawn(move || {
        join_coordinator.join_generation(&join_account, generation, Duration::from_secs(2), 1_001)
    });
    coordinator
        .reconcile_catalog([catalog_entry(&account, "revision-b")], 1_002)
        .unwrap();
    let error = joiner.join().unwrap().unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::CatalogRevoked);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(coordinator.is_idle());

    executor.release(1);
    executor.wait_idle();
    let current = coordinator.current(&account, 1_003).unwrap();
    assert_eq!(current.phase, UsageRefreshPhase::Idle);
    assert!(current.snapshot.is_none());
}

#[test]
fn catalog_purge_prevents_restart_resurrection() {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(FileAccountStateStore::at(temp.path().join("accounts")));
    let account = capability("account-a");
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let first = UsageCoordinator::new(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<FileAccountStateStore>::clone(&store),
        UsageCoordinatorConfig::default(),
    );
    let generation = first
        .request_refresh(&account, 0, true, 1_000)
        .unwrap()
        .generation;
    assert_eq!(
        join_ok(&first, &account, generation, 1_001).phase,
        UsageRefreshPhase::Completed
    );
    drop(first);

    let second = UsageCoordinator::with_catalog(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<FileAccountStateStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "revision-a")],
    );
    second.reconcile_catalog([], 1_002).unwrap();
    assert_eq!(store.load(&account, 1_002).unwrap(), None);
    drop(second);

    let restarted = UsageCoordinator::with_catalog(
        Arc::<ImmediateExecutor>::clone(&executor),
        store,
        UsageCoordinatorConfig::default(),
        std::iter::empty::<UsageCatalogEntry>(),
    );
    assert_eq!(
        restarted.current(&account, 1_003).unwrap_err().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
}
