// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::state::PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION;
use super::*;

struct ResetWriteFailureStore {
    inner: MemoryStore,
    fail_reset_write: AtomicUsize,
}

impl AccountStateStore for ResetWriteFailureStore {
    fn load(
        &self,
        capability: &UsageAccountCapability,
        now_epoch: i64,
    ) -> Result<Option<AccountStateEnvelope>, StateStoreError> {
        self.inner.load(capability, now_epoch)
    }

    fn store(
        &self,
        envelope: &AccountStateEnvelope,
        now_epoch: i64,
    ) -> Result<(), StateStoreError> {
        if envelope.phase == UsageRefreshPhase::Idle
            && envelope.retry_deadline_epoch == Some(5_000)
            && self.fail_reset_write.swap(0, Ordering::SeqCst) > 0
        {
            return Err(StateStoreError::Unavailable);
        }
        self.inner.store(envelope, now_epoch)
    }

    fn purge(&self, capability: &UsageAccountCapability) -> Result<(), StateStoreError> {
        self.inner.purge(capability)
    }
}

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
    let account = UsageAccountCapability {
        account_id: "account-a".into(),
        surface_id: "openai".into(),
    };

    coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    executor.release(1);
    let first = join_ok(&coordinator, &account, 1, 1_001);
    assert!(
        first
            .retry_at_epoch
            .is_some_and(|deadline| (1_030..=1_045).contains(&deadline))
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
            .is_some_and(|deadline| (second_start + 60..=second_start + 90).contains(&deadline))
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
fn claude_attempt_floor_persists_across_restart_and_force_cannot_bypass() {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(FileAccountStateStore::at(temp.path().join("accounts")));
    let scenarios = [
        (
            "needs-secret",
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::NeedsSecret,
                message: "host credential required".into(),
                retry_at_epoch: None,
            },
            true,
        ),
        (
            "unauthorized",
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::Unauthorized,
                message: "provider rejected credentials".into(),
                retry_at_epoch: None,
            },
            true,
        ),
        (
            "rate-limited-short",
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::RateLimited,
                message: "provider asked for an early retry".into(),
                retry_at_epoch: Some(1_100),
            },
            true,
        ),
        (
            "success",
            ProviderProbeOutcome::success(quota_view(1_000, 80)),
            false,
        ),
    ];

    for (id, initial_outcome, failed) in scenarios {
        let account = capability(id);
        let executor = Arc::new(ImmediateExecutor::new(initial_outcome));
        #[expect(
            clippy::clone_on_ref_ptr,
            reason = "coerce concrete executor to shared trait object"
        )]
        let provider_executor: Arc<dyn UsageProviderExecutor> = executor.clone();
        let coordinator = UsageCoordinator::new(
            provider_executor,
            Arc::<FileAccountStateStore>::clone(&store),
            UsageCoordinatorConfig {
                success_cooldown: Duration::from_secs(1),
                ..UsageCoordinatorConfig::default()
            },
        );
        let first = coordinator
            .request_refresh(&account, 0, true, 1_000)
            .unwrap();
        let terminal = join_ok(&coordinator, &account, first.generation, 1_001);
        if failed {
            assert_eq!(terminal.phase, UsageRefreshPhase::Failed);
        } else {
            assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
        }
        let invocation = store
            .load(&account, 1_001)
            .unwrap()
            .unwrap()
            .provider_invoked_at_epoch
            .expect("provider invocation was persisted");
        let attempt_floor = invocation.saturating_add(300);
        if failed {
            assert_eq!(terminal.retry_at_epoch, Some(attempt_floor));
        }
        drop(coordinator);

        let restarted_executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
            quota_view(attempt_floor, 79),
        )));
        #[expect(
            clippy::clone_on_ref_ptr,
            reason = "coerce concrete executor to shared trait object"
        )]
        let provider_executor: Arc<dyn UsageProviderExecutor> = restarted_executor.clone();
        let restarted = UsageCoordinator::new(
            provider_executor,
            Arc::<FileAccountStateStore>::clone(&store),
            UsageCoordinatorConfig {
                success_cooldown: Duration::from_secs(1),
                ..UsageCoordinatorConfig::default()
            },
        );
        assert_eq!(
            restarted
                .current(&account, attempt_floor - 1)
                .unwrap()
                .generation,
            1
        );
        assert_eq!(restarted.next_due_epoch(), Some(attempt_floor));
        assert!(restarted.poll_due(attempt_floor - 1).is_empty());
        let forced_early = restarted
            .request_refresh(&account, 1, true, attempt_floor - 1)
            .unwrap();
        assert_eq!(forced_early.generation, 1);
        assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);
        let allowed = restarted
            .request_refresh(&account, 1, true, attempt_floor)
            .unwrap();
        assert_eq!(allowed.generation, 2);
        assert_eq!(
            join_ok(&restarted, &account, 2, attempt_floor + 1).phase,
            UsageRefreshPhase::Completed
        );
        assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn claude_attempt_floor_starts_at_invocation_after_fake_clock_queue_delay() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(MemoryStore::default());
    let clock = Arc::new(FakeMonotonicClock::default());
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete ports to coordinator trait objects"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete ports to coordinator trait objects"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "share fake clock with the delayed-queue fixture"
    )]
    let clock_port: Arc<dyn MonotonicClock> = clock.clone();
    let coordinator = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig {
            max_concurrency: 1,
            queue_capacity: 4,
            ..UsageCoordinatorConfig::default()
        },
        None,
        None,
        clock_port,
    );
    let blocker = UsageAccountCapability {
        account_id: "queue-blocker".into(),
        surface_id: "openai".into(),
    };
    let account = capability("delayed-attempt");

    coordinator
        .request_refresh(&blocker, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    let queued = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    assert_eq!(queued.phase, UsageRefreshPhase::Queued);
    clock.advance(Duration::from_secs(400));
    executor.release(1);
    executor.wait_started(2);
    join_ok(&coordinator, &blocker, 1, 1_401);

    let first_invocation = store
        .states
        .lock()
        .unwrap()
        .get(&account)
        .unwrap()
        .provider_invoked_at_epoch
        .unwrap();
    assert_eq!(first_invocation, 1_400);
    executor.release(1);
    assert_eq!(
        join_ok(&coordinator, &account, 1, 1_401).phase,
        UsageRefreshPhase::Completed
    );

    let forced_early = coordinator
        .request_refresh(&account, 1, true, first_invocation + 299)
        .unwrap();
    assert_eq!(forced_early.generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    let allowed = coordinator
        .request_refresh(&account, 1, true, first_invocation + 300)
        .unwrap();
    assert_eq!(allowed.generation, 2);
    executor.wait_started(3);
    let second_invocation = store
        .states
        .lock()
        .unwrap()
        .get(&account)
        .unwrap()
        .provider_invoked_at_epoch
        .unwrap();
    assert!(second_invocation.saturating_sub(first_invocation) >= 300);
    executor.release(1);
    assert_eq!(
        join_ok(&coordinator, &account, 2, second_invocation + 1).phase,
        UsageRefreshPhase::Completed
    );
}

#[test]
fn account_state_v1_migration_preserves_results_and_enforces_fresh_attempt_floor() {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(FileAccountStateStore::at(temp.path().join("accounts")));
    let account = capability("legacy-attempt");
    let mut legacy = AccountStateEnvelope::idle(account.clone());
    legacy.schema_version = PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION;
    legacy.generation = 1;
    legacy.phase = UsageRefreshPhase::Completed;
    let last_good = quota_view(1_000, 80);
    legacy.terminal_result = Some(last_good.clone());
    legacy.last_good = Some(last_good);
    legacy.started_at_epoch = Some(1_000);
    legacy.completed_at_epoch = Some(1_001);
    legacy.success_deadline_epoch = Some(1_002);
    store.store(&legacy, 1_001).unwrap();
    let path = temp.path().join("accounts/claude-legacy-attempt.json");
    let mut bytes = serde_json::to_value(&legacy).unwrap();
    bytes["schema_version"] = serde_json::json!(PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION);
    bytes
        .as_object_mut()
        .unwrap()
        .remove("provider_invoked_at_epoch");
    std::fs::write(&path, serde_json::to_vec(&bytes).unwrap()).unwrap();

    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(2_300, 79),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to the coordinator port"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = executor.clone();
    let coordinator = UsageCoordinator::new(
        provider,
        Arc::<FileAccountStateStore>::clone(&store),
        UsageCoordinatorConfig::default(),
    );
    let restored = coordinator.current(&account, 2_000).unwrap();
    assert_eq!(restored.generation, 1);
    assert!(restored.snapshot.is_some());
    assert_eq!(
        store
            .load(&account, 2_000)
            .unwrap()
            .unwrap()
            .provider_invoked_at_epoch,
        Some(2_000)
    );

    let forced_early = coordinator
        .request_refresh(&account, 1, true, 2_299)
        .unwrap();
    assert_eq!(forced_early.generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    let allowed = coordinator
        .request_refresh(&account, 1, true, 2_300)
        .unwrap();
    assert_eq!(allowed.generation, 2);
    assert_eq!(
        join_ok(&coordinator, &account, 2, 2_301).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn catalog_revision_retains_retry_after_across_restart() {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(FileAccountStateStore::at(temp.path().join("accounts")));
    let account = capability("account-a");
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::RateLimited,
        message: "provider rate limited".into(),
        retry_at_epoch: Some(5_000),
    }));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to shared trait object"
    )]
    let provider_executor: Arc<dyn UsageProviderExecutor> = executor.clone();
    let coordinator = UsageCoordinator::with_catalog(
        provider_executor,
        Arc::<FileAccountStateStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "credential-revision-a")],
    );
    let first = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    let failed = join_ok(&coordinator, &account, first.generation, 1_001);
    assert_eq!(failed.retry_at_epoch, Some(5_000));

    coordinator
        .reconcile_catalog([catalog_entry(&account, "credential-revision-b")], 2_000)
        .unwrap();
    let reset = coordinator.current(&account, 2_000).unwrap();
    assert_eq!(reset.phase, UsageRefreshPhase::Idle);
    assert_eq!(reset.generation, 2);
    let durable = store.load(&account, 2_000).unwrap().unwrap();
    assert_eq!(durable.started_at_epoch, Some(1_000));
    assert_eq!(durable.rate_limit_deadline_epoch, Some(5_000));
    assert_eq!(durable.retry_deadline_epoch, Some(5_000));
    drop(coordinator);

    let restarted_executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(5_000, 75),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to shared trait object"
    )]
    let provider_executor: Arc<dyn UsageProviderExecutor> = restarted_executor.clone();
    let restarted = UsageCoordinator::with_catalog(
        provider_executor,
        store,
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "credential-revision-b")],
    );
    let restored = restarted.current(&account, 4_999).unwrap();
    assert_eq!(restored.generation, 2);
    assert_eq!(restored.retry_at_epoch, Some(5_000));
    let forced_early = restarted.request_refresh(&account, 2, true, 4_999).unwrap();
    assert_eq!(forced_early.generation, 2);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);
    let allowed = restarted.request_refresh(&account, 2, true, 5_000).unwrap();
    assert_eq!(allowed.generation, 3);
    assert_eq!(
        join_ok(&restarted, &account, 3, 5_001).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_catalog_reset_write_restores_retry_state_and_catalog() {
    let store = Arc::new(ResetWriteFailureStore {
        inner: MemoryStore::default(),
        fail_reset_write: AtomicUsize::new(1),
    });
    let account = capability("account-a");
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::RateLimited,
        message: "provider rate limited".into(),
        retry_at_epoch: Some(5_000),
    }));
    let coordinator = UsageCoordinator::with_catalog(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<ResetWriteFailureStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "credential-revision-a")],
    );
    let first = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    assert_eq!(
        join_ok(&coordinator, &account, first.generation, 1_001).retry_at_epoch,
        Some(5_000)
    );

    let error = coordinator
        .reconcile_catalog([catalog_entry(&account, "credential-revision-b")], 2_000)
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    coordinator
        .reconcile_catalog([catalog_entry(&account, "credential-revision-a")], 2_001)
        .unwrap();
    let current = coordinator.current(&account, 2_001).unwrap();
    assert_eq!(current.phase, UsageRefreshPhase::Failed);
    assert_eq!(current.generation, 1);
    assert_eq!(current.retry_at_epoch, Some(5_000));
    let durable = store.load(&account, 2_001).unwrap().unwrap();
    assert_eq!(durable.phase, UsageRefreshPhase::Failed);
    assert_eq!(durable.retry_deadline_epoch, Some(5_000));
    assert_eq!(durable.rate_limit_deadline_epoch, Some(5_000));
    let forced_early = coordinator
        .request_refresh(&account, 1, true, 4_999)
        .unwrap();
    assert_eq!(forced_early.generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
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
    abandoned.provider_invoked_at_epoch = Some(1_000);
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
    assert_eq!(recovered.generation, 4);
    assert_eq!(recovered.phase, UsageRefreshPhase::Failed);
    assert_eq!(recovered.retry_at_epoch, Some(1_300));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    let recovered = coordinator
        .request_refresh(&account, 0, true, 1_300)
        .unwrap();
    executor.wait_started(1);
    let joiner = coordinator
        .request_refresh(&account, 0, true, 1_300)
        .unwrap();
    assert_eq!(recovered.generation, 5);
    assert_eq!(joiner.generation, 5);
    assert!(joiner.phase.is_active());

    executor.release(1);
    let terminal = join_ok(&coordinator, &account, 5, 1_301);
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
        .request_refresh(&account, first.generation, true, 1_400)
        .unwrap();
    executor.wait_started(2);
    coordinator
        .reconcile_catalog([], 1_401)
        .expect("catalog removal is durable");
    let revoked = coordinator.current(&account, 1_401).unwrap();
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
                1_401,
            )
            .unwrap_err()
            .kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
    assert!(store.purges.lock().unwrap().contains(&account));

    executor.release(1);
    executor.wait_idle();
    let after_late_result = coordinator.current(&account, 1_402).unwrap();
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
            .request_refresh(&account, revoked.generation, true, 1_402)
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
        .request_refresh(&account, reset.generation, true, 1_400)
        .unwrap();
    assert_eq!(
        join_ok(&coordinator, &account, next.generation, 1_401).phase,
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
