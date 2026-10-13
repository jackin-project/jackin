// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::state::LEGACY_ACCOUNT_STATE_SCHEMA_VERSION;
use super::*;

#[test]
fn system_clock_samples_real_wall_and_monotonic_domains() {
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    let clock = SystemMonotonicClock::default();
    let sample = clock.sample(1);
    let after = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();

    assert!(before <= sample.wall_epoch);
    assert!(sample.wall_epoch <= after);
    assert!(sample.monotonic <= clock.now());
}

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

struct AfterApplyFailureStore {
    inner: MemoryStore,
    store_calls: AtomicUsize,
    fail_store_on_call: AtomicUsize,
    fail_purge_after_apply: AtomicUsize,
    purge_calls: AtomicUsize,
}

impl AccountStateStore for AfterApplyFailureStore {
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
        self.inner.store(envelope, now_epoch)?;
        let call = self.store_calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self
            .fail_store_on_call
            .compare_exchange(call, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Err(StateStoreError::Unavailable);
        }
        Ok(())
    }

    fn purge(&self, capability: &UsageAccountCapability) -> Result<(), StateStoreError> {
        self.purge_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.purge(capability)?;
        if self.fail_purge_after_apply.swap(0, Ordering::SeqCst) > 0 {
            return Err(StateStoreError::Unavailable);
        }
        Ok(())
    }
}

struct PanicAfterDurableStore {
    inner: FileAccountStateStore,
    panic_after_store: AtomicUsize,
    purge_calls: AtomicUsize,
}

impl AccountStateStore for PanicAfterDurableStore {
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
        self.inner.store(envelope, now_epoch)?;
        if self.panic_after_store.swap(0, Ordering::SeqCst) > 0 {
            panic!("injected crash after durable catalog marker");
        }
        Ok(())
    }

    fn purge(&self, capability: &UsageAccountCapability) -> Result<(), StateStoreError> {
        self.purge_calls.fetch_add(1, Ordering::SeqCst);
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
        let coordinator = coordinator_with_fake_clock(
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
        let restarted_clock = Arc::new(ManualClock::at(
            u64::try_from(attempt_floor - 1).expect("attempt floor is positive"),
        ));
        #[expect(
            clippy::clone_on_ref_ptr,
            reason = "share the controllable clock with the coordinator"
        )]
        let clock_port: Arc<dyn MonotonicClock> = restarted_clock.clone();
        let restarted = UsageCoordinator::start_with_clock(
            provider_executor,
            Arc::<FileAccountStateStore>::clone(&store),
            UsageCoordinatorConfig {
                success_cooldown: Duration::from_secs(1),
                ..UsageCoordinatorConfig::default()
            },
            None,
            None,
            clock_port,
        );
        assert_eq!(
            restarted
                .current(&account, attempt_floor - 1)
                .unwrap()
                .generation,
            1
        );
        // Recovery installs a fresh 300-second monotonic floor even when the
        // prior wall deadline is one second away.
        assert_eq!(restarted.next_due_epoch(), Some(attempt_floor + 299));
        assert!(restarted.poll_due(attempt_floor - 1).is_empty());
        let forced_early = restarted
            .request_refresh(&account, 1, true, attempt_floor - 1)
            .unwrap();
        assert_eq!(forced_early.generation, 1);
        assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);
        restarted_clock.advance(Duration::from_secs(1));
        let at_persisted_deadline = restarted
            .request_refresh(&account, 1, true, attempt_floor)
            .unwrap();
        assert_eq!(at_persisted_deadline.generation, 1);
        assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);

        restarted_clock.advance(Duration::from_secs(298));
        let before_reload_fence = restarted
            .request_refresh(&account, 1, true, attempt_floor + 298)
            .unwrap();
        assert_eq!(before_reload_fence.generation, 1);
        assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);

        restarted_clock.advance(Duration::from_secs(1));
        let allowed = restarted
            .request_refresh(&account, 1, true, attempt_floor + 299)
            .unwrap();
        assert_eq!(allowed.generation, 2);
        assert_eq!(
            join_ok(&restarted, &account, 2, attempt_floor + 300).phase,
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
    legacy.schema_version = LEGACY_ACCOUNT_STATE_SCHEMA_VERSION;
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
    bytes["schema_version"] = serde_json::json!(LEGACY_ACCOUNT_STATE_SCHEMA_VERSION);
    bytes
        .as_object_mut()
        .unwrap()
        .remove("provider_invoked_at_epoch");
    bytes
        .as_object_mut()
        .unwrap()
        .remove("reload_fence_required");
    std::fs::write(&path, serde_json::to_vec(&bytes).unwrap()).unwrap();

    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(2_300, 79),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to the coordinator port"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = executor.clone();
    let coordinator = coordinator_with_fake_clock(
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
        None
    );
    assert!(
        store
            .load(&account, 2_000)
            .unwrap()
            .unwrap()
            .reload_fence_required
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
    let coordinator = catalog_coordinator_with_fake_clock(
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
    assert_eq!(durable.started_at_epoch, None);
    assert_eq!(durable.provider_invoked_at_epoch, Some(1_000));
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
    let restarted_clock = Arc::new(ManualClock::at(2_000));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete store to the coordinator state port"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "share the controllable clock with the coordinator"
    )]
    let clock_port: Arc<dyn MonotonicClock> = restarted_clock.clone();
    let restarted = UsageCoordinator::start_with_clock(
        provider_executor,
        state_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(
            account.clone(),
            "credential-revision-b".into(),
        )])),
        None,
        clock_port,
    );
    let restored = restarted.current(&account, 2_000).unwrap();
    assert_eq!(restored.generation, 2);
    assert_eq!(restored.retry_at_epoch, Some(5_000));
    let forced_early = restarted.request_refresh(&account, 2, true, 2_000).unwrap();
    assert_eq!(forced_early.generation, 2);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);

    // The 3,000-second persisted Retry-After is longer than the fresh
    // 300-second reload fence. It remains authoritative after monotonic time
    // has passed the reload fence.
    restarted_clock.advance(Duration::from_secs(2_999));
    let before_retry_deadline = restarted.request_refresh(&account, 2, true, 4_999).unwrap();
    assert_eq!(before_retry_deadline.generation, 2);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);
    restarted_clock.advance(Duration::from_secs(1));
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
    let coordinator = catalog_coordinator_with_fake_clock(
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
fn after_apply_marker_store_failure_restores_preimage_before_any_purge() {
    let store = Arc::new(AfterApplyFailureStore {
        inner: MemoryStore::default(),
        store_calls: AtomicUsize::new(0),
        fail_store_on_call: AtomicUsize::new(0),
        fail_purge_after_apply: AtomicUsize::new(0),
        purge_calls: AtomicUsize::new(0),
    });
    let account = capability("after-apply-marker-store");
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = catalog_coordinator_with_fake_clock(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<AfterApplyFailureStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "revision-a")],
    );
    let started = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    assert_eq!(
        join_ok(&coordinator, &account, started.generation, 1_001).phase,
        UsageRefreshPhase::Completed
    );
    let before = store.inner.states.lock().unwrap().get(&account).cloned();
    store.fail_store_on_call.store(
        store.store_calls.load(Ordering::SeqCst) + 1,
        Ordering::SeqCst,
    );

    let error = coordinator
        .reconcile_catalog([catalog_entry(&account, "revision-b")], 1_002)
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(store.purge_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        store.inner.states.lock().unwrap().get(&account).cloned(),
        before
    );
    assert_eq!(
        coordinator.current(&account, 1_002).unwrap().phase,
        UsageRefreshPhase::Completed
    );
}

#[test]
fn failed_reconcile_does_not_erase_marker_when_store_preimage_was_missing() {
    let store = Arc::new(AfterApplyFailureStore {
        inner: MemoryStore::default(),
        store_calls: AtomicUsize::new(0),
        fail_store_on_call: AtomicUsize::new(0),
        fail_purge_after_apply: AtomicUsize::new(0),
        purge_calls: AtomicUsize::new(0),
    });
    let account = capability("missing-preimage-marker");
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = catalog_coordinator_with_fake_clock(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<AfterApplyFailureStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "revision-a")],
    );
    let started = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    assert_eq!(
        join_ok(&coordinator, &account, started.generation, 1_001).phase,
        UsageRefreshPhase::Completed
    );

    // Simulate a missing durable preimage while the live coordinator retains
    // evidence that a provider invocation occurred.
    store.inner.purge(&account).unwrap();
    store.fail_store_on_call.store(
        store.store_calls.load(Ordering::SeqCst) + 1,
        Ordering::SeqCst,
    );
    let error = coordinator
        .reconcile_catalog([catalog_entry(&account, "revision-b")], 1_002)
        .unwrap_err();

    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(store.purge_calls.load(Ordering::SeqCst), 0);
    let marker = store.inner.states.lock().unwrap().get(&account).cloned();
    let marker = marker.expect("rollback must retain the materialized cooldown marker");
    assert_eq!(marker.phase, UsageRefreshPhase::Idle);
    assert_eq!(marker.provider_invoked_at_epoch, Some(1_000));
    assert!(
        marker
            .retry_deadline_epoch
            .is_some_and(|deadline| deadline >= 1_300)
    );
    assert!(marker.terminal_result.is_none());
    assert!(marker.last_good.is_none());
    assert!(marker.terminal_error.is_none());
    assert_eq!(
        coordinator.current(&account, 1_002).unwrap().phase,
        UsageRefreshPhase::Completed
    );
}

#[test]
fn after_apply_purge_failure_restores_preimage_and_catalog() {
    let store = Arc::new(AfterApplyFailureStore {
        inner: MemoryStore::default(),
        store_calls: AtomicUsize::new(0),
        fail_store_on_call: AtomicUsize::new(0),
        fail_purge_after_apply: AtomicUsize::new(0),
        purge_calls: AtomicUsize::new(0),
    });
    let account = capability("after-apply-purge");
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = catalog_coordinator_with_fake_clock(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<AfterApplyFailureStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "revision-a")],
    );
    let started = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    assert_eq!(
        join_ok(&coordinator, &account, started.generation, 1_001).phase,
        UsageRefreshPhase::Completed
    );
    let before = store.inner.states.lock().unwrap().get(&account).cloned();
    store.fail_purge_after_apply.store(1, Ordering::SeqCst);

    // The 300-second attempt floor has elapsed, so this exercises a real
    // purge rather than the result-free cooldown-marker path.
    let error = coordinator.reconcile_catalog([], 10_000).unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(store.purge_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        store.inner.states.lock().unwrap().get(&account).cloned(),
        before
    );
    assert_eq!(
        coordinator.current(&account, 10_000).unwrap().phase,
        UsageRefreshPhase::Completed
    );

    coordinator.reconcile_catalog([], 10_001).unwrap();
    assert_eq!(store.purge_calls.load(Ordering::SeqCst), 2);
    let revoked = coordinator.current(&account, 10_001).unwrap();
    assert_eq!(revoked.phase, UsageRefreshPhase::Failed);
    assert_eq!(
        revoked.error.unwrap().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
}

#[test]
fn later_marker_store_failure_rolls_back_prior_marker_before_any_purge() {
    let store = Arc::new(AfterApplyFailureStore {
        inner: MemoryStore::default(),
        store_calls: AtomicUsize::new(0),
        fail_store_on_call: AtomicUsize::new(0),
        fail_purge_after_apply: AtomicUsize::new(0),
        purge_calls: AtomicUsize::new(0),
    });
    let first = capability("multi-marker-a-first");
    let failing = capability("multi-marker-z-fails");
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = catalog_coordinator_with_fake_clock(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<AfterApplyFailureStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [
            catalog_entry(&failing, "revision-a"),
            catalog_entry(&first, "revision-a"),
        ],
    );

    let first_refresh = coordinator.request_refresh(&first, 0, true, 1_000).unwrap();
    assert_eq!(
        join_ok(&coordinator, &first, first_refresh.generation, 1_001).phase,
        UsageRefreshPhase::Completed
    );
    let failing_refresh = coordinator
        .request_refresh(&failing, 0, true, 1_001)
        .unwrap();
    assert_eq!(
        join_ok(&coordinator, &failing, failing_refresh.generation, 1_002).phase,
        UsageRefreshPhase::Completed
    );
    let before = store.inner.states.lock().unwrap().clone();

    // BTree order writes the first account's marker before the selected
    // second write applies and fails. Both touched files must be restored.
    store.fail_store_on_call.store(
        store.store_calls.load(Ordering::SeqCst) + 2,
        Ordering::SeqCst,
    );
    let error = coordinator.reconcile_catalog([], 1_003).unwrap_err();

    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(store.purge_calls.load(Ordering::SeqCst), 0);
    assert_eq!(store.inner.states.lock().unwrap().clone(), before);
    assert_eq!(
        coordinator.current(&first, 1_003).unwrap().phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(
        coordinator.current(&failing, 1_003).unwrap().phase,
        UsageRefreshPhase::Completed
    );
}

#[test]
fn crash_after_durable_cooldown_marker_precedes_purge_and_survives_restart() {
    let temp = tempfile::tempdir().unwrap();
    let account = capability("crash-after-marker");
    let store = Arc::new(PanicAfterDurableStore {
        inner: FileAccountStateStore::at(temp.path().join("accounts")),
        panic_after_store: AtomicUsize::new(0),
        purge_calls: AtomicUsize::new(0),
    });
    let clock = Arc::new(ManualClock::at(1_000));
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
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
        reason = "share the controllable clock with the coordinator"
    )]
    let clock_port: Arc<dyn MonotonicClock> = clock.clone();
    let coordinator = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-a".into())])),
        None,
        clock_port,
    );
    let started = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    assert_eq!(
        join_ok(&coordinator, &account, started.generation, 1_001).phase,
        UsageRefreshPhase::Completed
    );

    store.panic_after_store.store(1, Ordering::SeqCst);
    let crash = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drop(coordinator.reconcile_catalog([], 1_002));
    }));
    assert!(
        crash.is_err(),
        "injected crash must occur after marker write"
    );
    drop(coordinator);
    assert_eq!(store.purge_calls.load(Ordering::SeqCst), 0);

    let tombstone = store
        .load(&account, 1_002)
        .unwrap()
        .expect("the marker is durable before a catalog purge can begin");
    assert_eq!(tombstone.phase, UsageRefreshPhase::Idle);
    assert_eq!(tombstone.provider_invoked_at_epoch, Some(1_000));
    assert!(tombstone.retry_deadline_epoch.unwrap_or_default() >= 1_300);
    assert!(tombstone.terminal_result.is_none());
    assert!(tombstone.last_good.is_none());
    assert!(tombstone.terminal_error.is_none());

    // The process died before the catalog publication. Restarting with the
    // old catalog still cannot spend the durable marker's actual-call floor
    // via a forward wall-clock jump.
    let restarted_clock = Arc::new(ManualClock::at(50_000));
    let restarted_executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(100_301, 80),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to coordinator trait object"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = restarted_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete store to coordinator trait object"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "share the controllable clock with the coordinator"
    )]
    let clock_port: Arc<dyn MonotonicClock> = restarted_clock.clone();
    let restarted = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-a".into())])),
        None,
        clock_port,
    );
    let restored = restarted.current(&account, 50_000).unwrap();
    assert_eq!(restored.phase, UsageRefreshPhase::Idle);
    assert!(restored.snapshot.is_none());
    assert_eq!(restarted.next_due_epoch(), Some(50_300));
    restarted_clock.set_wall_epoch(100_000);
    assert_eq!(restarted.next_due_epoch(), Some(100_300));
    assert!(restarted.poll_due(100_299).is_empty());
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);
    restarted_clock.advance(Duration::from_secs(299));
    assert!(restarted.poll_due(100_299).is_empty());
    restarted_clock.advance(Duration::from_secs(1));
    let due = restarted.poll_due(100_300);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].generation, tombstone.generation + 1);
    assert_eq!(
        join_ok(&restarted, &account, due[0].generation, 100_301).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn corrupt_removed_claude_state_becomes_result_free_uncertainty_marker() {
    let store = Arc::new(MemoryStore::default());
    let account = capability("corrupt-removed-state");
    *store.load_error.lock().unwrap() = Some(StateStoreError::Corrupt);
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_300, 80),
    )));
    let coordinator = catalog_coordinator_with_fake_clock(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<MemoryStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "revision-a")],
    );

    coordinator.reconcile_catalog([], 1_000).unwrap();
    assert!(store.purges.lock().unwrap().is_empty());
    *store.load_error.lock().unwrap() = None;
    let marker = store.states.lock().unwrap().get(&account).cloned().unwrap();
    assert_eq!(marker.phase, UsageRefreshPhase::Idle);
    assert!(marker.reload_fence_required);
    assert_eq!(marker.provider_invoked_at_epoch, None);
    assert!(
        marker
            .retry_deadline_epoch
            .is_some_and(|deadline| deadline >= 1_300)
    );
    assert!(marker.terminal_result.is_none());
    assert!(marker.last_good.is_none());
    assert!(marker.terminal_error.is_none());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
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
    assert_eq!(recovered.retry_at_epoch, Some(1_301));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    let recovered = coordinator
        .request_refresh(&account, 0, true, 1_301)
        .unwrap();
    executor.wait_started(1);
    let joiner = coordinator
        .request_refresh(&account, 0, true, 1_301)
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
fn catalog_revocation_clears_materialized_data_and_fences_late_result() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(MemoryStore::default());
    let account = capability("account-a");
    let coordinator = catalog_coordinator_with_fake_clock(
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
    assert_eq!(revoked.snapshot, None);
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
    assert!(
        !store.purges.lock().unwrap().contains(&account),
        "an in-flight provider attempt must retain its durable cooldown tombstone"
    );
    let pending = store
        .states
        .lock()
        .unwrap()
        .get(&account)
        .cloned()
        .expect("in-flight attempt marker is durable before provider work returns");
    assert_eq!(pending.phase, UsageRefreshPhase::Updating);
    assert!(pending.terminal_result.is_none());
    assert!(pending.last_good.is_none());
    assert!(pending.terminal_error.is_none());

    executor.release(1);
    executor.wait_idle();
    let after_late_result = coordinator.current(&account, 1_402).unwrap();
    assert_eq!(
        after_late_result.error.as_ref().unwrap().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
    assert_eq!(after_late_result.generation, revoked.generation);
    assert_eq!(after_late_result.snapshot, None);
    assert_eq!(
        coordinator
            .request_refresh(&account, revoked.generation, true, 1_402)
            .unwrap_err()
            .kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
}

#[test]
fn claude_cooldown_uses_monotonic_time_when_wall_clock_jumps_forward() {
    let account = capability("clock-jump");
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(2_000, 80),
    )));
    let store = Arc::new(MemoryStore::default());
    let clock = Arc::new(ManualClock::at(2_000));
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
        reason = "share the controllable clock with the coordinator"
    )]
    let clock_port: Arc<dyn MonotonicClock> = clock.clone();
    let coordinator = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-a".into())])),
        None,
        clock_port,
    );

    let first = coordinator
        .request_refresh(&account, 0, true, 2_000)
        .unwrap();
    assert_eq!(
        join_ok(&coordinator, &account, first.generation, 2_000).phase,
        UsageRefreshPhase::Completed
    );

    clock.set_wall_epoch(2_500);
    coordinator
        .reconcile_catalog([catalog_entry(&account, "revision-b")], 2_500)
        .unwrap();
    let after_forward_jump = coordinator
        .request_refresh(&account, first.generation, true, 2_500)
        .unwrap();
    assert_eq!(after_forward_jump.generation, first.generation + 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    clock.advance(Duration::from_secs(299));
    let before_monotonic_floor = coordinator
        .request_refresh(&account, after_forward_jump.generation, true, 2_799)
        .unwrap();
    assert_eq!(
        before_monotonic_floor.generation,
        after_forward_jump.generation
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    clock.advance(Duration::from_secs(1));
    let allowed = coordinator
        .request_refresh(&account, after_forward_jump.generation, true, 2_800)
        .unwrap();
    assert_eq!(allowed.generation, after_forward_jump.generation + 1);
    assert_eq!(
        join_ok(&coordinator, &account, allowed.generation, 2_800).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn restart_reload_fence_blocks_forward_wall_jump_but_not_fresh_accounts() {
    let account = capability("restart-fenced");
    let fresh_account = capability("fresh-account");
    let store = Arc::new(MemoryStore::default());
    let initial_executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let initial_clock = Arc::new(ManualClock::at(1_000));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete ports to coordinator trait objects"
    )]
    let initial_provider: Arc<dyn UsageProviderExecutor> = initial_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete ports to coordinator trait objects"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "share the controllable clock with the coordinator"
    )]
    let initial_clock_port: Arc<dyn MonotonicClock> = initial_clock.clone();
    let initial = UsageCoordinator::start_with_clock(
        initial_provider,
        state_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-a".into())])),
        None,
        initial_clock_port,
    );
    let first = initial.request_refresh(&account, 0, true, 1_000).unwrap();
    assert_eq!(
        join_ok(&initial, &account, first.generation, 1_000).phase,
        UsageRefreshPhase::Completed
    );
    let persisted = store.load(&account, 1_000).unwrap().unwrap();
    assert_eq!(persisted.provider_invoked_at_epoch, Some(1_000));
    drop(initial);

    // A fresh monotonic origin models restart/reboot where no trustworthy
    // boot-relative clock provenance survived. The wall clock is already
    // beyond the old deadline, so only the conservative reload fence blocks.
    let restarted_executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(2_800, 79),
    )));
    let restarted_clock = Arc::new(ManualClock::at(2_000));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete ports to coordinator trait objects"
    )]
    let restarted_provider: Arc<dyn UsageProviderExecutor> = restarted_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete ports to coordinator trait objects"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "share the controllable clock with the coordinator"
    )]
    let restarted_clock_port: Arc<dyn MonotonicClock> = restarted_clock.clone();
    let restarted = UsageCoordinator::start_with_clock(
        restarted_provider,
        state_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([
            (account.clone(), "revision-a".into()),
            (fresh_account.clone(), "revision-a".into()),
        ])),
        None,
        restarted_clock_port,
    );
    let initially_suppressed = restarted
        .request_refresh(&account, first.generation, true, 2_000)
        .unwrap();
    assert_eq!(initially_suppressed.generation, first.generation);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);

    let fresh = restarted
        .request_refresh(&fresh_account, 0, true, 2_000)
        .unwrap();
    assert_eq!(fresh.generation, 1);
    assert_eq!(
        join_ok(&restarted, &fresh_account, fresh.generation, 2_000).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 1);

    restarted_clock.set_wall_epoch(2_500);
    let after_forward_jump = restarted
        .request_refresh(&account, first.generation, true, 2_500)
        .unwrap();
    assert_eq!(after_forward_jump.generation, first.generation);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 1);

    restarted_clock.advance(Duration::from_secs(299));
    let before_reload_fence = restarted
        .request_refresh(&account, first.generation, true, 2_799)
        .unwrap();
    assert_eq!(before_reload_fence.generation, first.generation);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 1);

    restarted_clock.advance(Duration::from_secs(1));
    let allowed = restarted
        .request_refresh(&account, first.generation, true, 2_800)
        .unwrap();
    assert_eq!(allowed.generation, first.generation + 1);
    assert_eq!(
        join_ok(&restarted, &account, allowed.generation, 2_800).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        store
            .load(&account, 2_800)
            .unwrap()
            .unwrap()
            .provider_invoked_at_epoch,
        Some(2_800),
        "the reload wait must not rewrite the actual invocation timestamp"
    );
}

#[test]
fn unresolved_updating_marker_gets_reload_fence_without_fabricated_invocation() {
    let account = capability("unresolved-reload");
    let mut unresolved = AccountStateEnvelope::idle(account.clone());
    unresolved.generation = 7;
    unresolved.phase = UsageRefreshPhase::Updating;
    unresolved.started_at_epoch = Some(1_000);
    assert_eq!(unresolved.provider_invoked_at_epoch, None);
    let store = Arc::new(MemoryStore::default());
    store
        .states
        .lock()
        .unwrap()
        .insert(account.clone(), unresolved);
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(2_800, 78),
    )));
    let clock = Arc::new(ManualClock::at(2_000));
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
        reason = "share the controllable clock with the coordinator"
    )]
    let clock_port: Arc<dyn MonotonicClock> = clock.clone();
    let coordinator = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-a".into())])),
        None,
        clock_port,
    );

    let recovered = coordinator
        .request_refresh(&account, 0, true, 2_000)
        .unwrap();
    assert_eq!(recovered.generation, 7);
    assert_eq!(recovered.phase, UsageRefreshPhase::Failed);
    assert_eq!(recovered.retry_at_epoch, Some(2_300));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        store
            .load(&account, 2_000)
            .unwrap()
            .unwrap()
            .provider_invoked_at_epoch,
        None,
        "an unresolved attempt is not evidence of an actual provider call"
    );
    assert!(
        store
            .load(&account, 2_000)
            .unwrap()
            .unwrap()
            .reload_fence_required
    );

    coordinator
        .reconcile_catalog([catalog_entry(&account, "revision-b")], 2_001)
        .unwrap();
    let reset_marker = store.load(&account, 2_001).unwrap().unwrap();
    assert_eq!(reset_marker.phase, UsageRefreshPhase::Idle);
    assert_eq!(reset_marker.provider_invoked_at_epoch, None);
    assert!(reset_marker.reload_fence_required);
    drop(coordinator);

    // The reload fence is itself durable safety state. It must survive even
    // when no wall-clock retry/cooldown deadline remains to justify a tombstone.
    {
        let mut states = store.states.lock().unwrap();
        let uncertain = states.get_mut(&account).unwrap();
        uncertain.rate_limit_deadline_epoch = None;
        uncertain.retry_deadline_epoch = None;
        uncertain.success_deadline_epoch = None;
    }
    let removal_executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(3_300, 78),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to coordinator trait object"
    )]
    let removal_provider: Arc<dyn UsageProviderExecutor> = removal_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete store to coordinator trait object"
    )]
    let removal_store: Arc<dyn AccountStateStore> = store.clone();
    let removal_clock = Arc::new(ManualClock::at(2_002));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "share the controllable clock with the coordinator"
    )]
    let removal_clock_port: Arc<dyn MonotonicClock> = removal_clock.clone();
    let removal_coordinator = UsageCoordinator::start_with_clock(
        removal_provider,
        removal_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-b".into())])),
        None,
        removal_clock_port,
    );
    removal_coordinator.reconcile_catalog([], 2_002).unwrap();
    let purged_marker = store.load(&account, 2_002).unwrap().unwrap();
    assert_eq!(purged_marker.phase, UsageRefreshPhase::Idle);
    assert!(purged_marker.terminal_result.is_none());
    assert!(purged_marker.last_good.is_none());
    assert_eq!(purged_marker.provider_invoked_at_epoch, None);
    assert!(purged_marker.reload_fence_required);
    assert_eq!(purged_marker.rate_limit_deadline_epoch, None);
    assert!(
        purged_marker
            .retry_deadline_epoch
            .is_some_and(|deadline| deadline >= 2_302)
    );
    assert_eq!(purged_marker.success_deadline_epoch, None);
    drop(removal_coordinator);

    // Clearing the wall projection after removal proves the persisted
    // uncertainty marker independently reinstalls a monotonic fence on load.
    {
        let mut states = store.states.lock().unwrap();
        let uncertain = states.get_mut(&account).unwrap();
        uncertain.rate_limit_deadline_epoch = None;
        uncertain.retry_deadline_epoch = None;
        uncertain.success_deadline_epoch = None;
    }

    // Restart again while uncertainty remains. A fresh monotonic origin models
    // unavailable boot-clock provenance; moving wall time forward cannot
    // consume the new process's conservative fence.
    let restarted_clock = Arc::new(ManualClock::at(2_500));
    let restarted_executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(3_300, 78),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to coordinator trait objects"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = restarted_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete store to the coordinator state port"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "share the controllable clock with the coordinator"
    )]
    let restarted_clock_port: Arc<dyn MonotonicClock> = restarted_clock.clone();
    let restarted = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-c".into())])),
        None,
        restarted_clock_port,
    );
    let after_reload = restarted
        .request_refresh(&account, purged_marker.generation, true, 2_500)
        .unwrap();
    assert_eq!(after_reload.generation, purged_marker.generation);
    assert_eq!(after_reload.phase, UsageRefreshPhase::Idle);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);

    restarted_clock.set_wall_epoch(3_000);
    let after_forward_jump = restarted
        .request_refresh(&account, after_reload.generation, true, 3_000)
        .unwrap();
    assert_eq!(after_forward_jump.generation, after_reload.generation);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);
    restarted_clock.advance(Duration::from_secs(299));
    let before_reload_fence = restarted
        .request_refresh(&account, after_reload.generation, true, 3_299)
        .unwrap();
    assert_eq!(before_reload_fence.generation, after_reload.generation);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);

    restarted_clock.advance(Duration::from_secs(1));
    let allowed = restarted
        .request_refresh(&account, after_reload.generation, true, 3_300)
        .unwrap();
    assert_eq!(allowed.generation, after_reload.generation + 1);
    assert_eq!(
        join_ok(&restarted, &account, allowed.generation, 3_300).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 1);
    let invoked = store.load(&account, 3_300).unwrap().unwrap();
    assert_eq!(invoked.provider_invoked_at_epoch, Some(3_300));
    assert!(!invoked.reload_fence_required);
}

#[test]
fn catalog_revision_keeps_pending_marker_and_completion_floor_across_readd() {
    let account = capability("pending-revision");
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(MemoryStore::default());
    let clock = Arc::new(ManualClock::at(1_000));
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
        reason = "share the controllable clock with the coordinator"
    )]
    let clock_port: Arc<dyn MonotonicClock> = clock.clone();
    let coordinator = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-a".into())])),
        None,
        clock_port,
    );

    let started = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    let dispatch_marker = store
        .states
        .lock()
        .unwrap()
        .get(&account)
        .cloned()
        .expect("Updating state must be durable before dispatch");
    assert_eq!(dispatch_marker.phase, UsageRefreshPhase::Updating);
    assert!(dispatch_marker.terminal_result.is_none());
    assert!(dispatch_marker.last_good.is_none());

    coordinator
        .reconcile_catalog([catalog_entry(&account, "revision-b")], 1_010)
        .unwrap();
    let revision_marker = store
        .states
        .lock()
        .unwrap()
        .get(&account)
        .cloned()
        .expect("catalog revision must persist a result-free pending marker");
    assert_eq!(revision_marker.phase, UsageRefreshPhase::Updating);
    assert!(revision_marker.terminal_result.is_none());
    assert!(revision_marker.last_good.is_none());
    assert!(revision_marker.terminal_error.is_none());

    let readded = coordinator
        .request_refresh(&account, started.generation + 1, true, 1_011)
        .unwrap();
    assert_eq!(readded.phase, UsageRefreshPhase::Idle);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    clock.advance(Duration::from_secs(20));
    executor.release(1);
    executor.wait_idle();
    // The executor's active count reaches zero just before the coordinator
    // worker commits the fenced completion. Observe the durable state rather
    // than treating provider return as completion of the whole transaction.
    let persistence_deadline = Instant::now() + Duration::from_secs(2);
    let completed_tombstone = loop {
        let persisted = store.states.lock().unwrap().get(&account).cloned();
        if persisted.as_ref().is_some_and(|envelope| {
            envelope.phase == UsageRefreshPhase::Idle
                && envelope.success_deadline_epoch == Some(1_320)
        }) {
            break persisted.expect("checked durable completion marker");
        }
        assert!(
            Instant::now() < persistence_deadline,
            "fenced completion did not persist its cooldown"
        );
        std::thread::yield_now();
    };
    assert_eq!(completed_tombstone.phase, UsageRefreshPhase::Idle);
    assert_eq!(completed_tombstone.success_deadline_epoch, Some(1_320));
    assert!(completed_tombstone.terminal_result.is_none());
    assert!(completed_tombstone.last_good.is_none());

    clock.advance(Duration::from_secs(299));
    let before_floor = coordinator
        .request_refresh(&account, readded.generation, true, 1_319)
        .unwrap();
    assert_eq!(before_floor.generation, readded.generation);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    clock.advance(Duration::from_secs(1));
    let after_floor = coordinator
        .request_refresh(&account, readded.generation, true, 1_320)
        .unwrap();
    assert_eq!(after_floor.generation, readded.generation + 1);
    executor.wait_started(2);
    executor.release(1);
    assert_eq!(
        join_ok(&coordinator, &account, after_floor.generation, 1_321).phase,
        UsageRefreshPhase::Completed
    );
}

#[test]
fn catalog_revision_change_stores_cooldown_marker_and_allows_new_revision() {
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(MemoryStore::default());
    let account = capability("account-a");
    let coordinator = catalog_coordinator_with_fake_clock(
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
    assert!(store.purges.lock().unwrap().is_empty());
    let marker = store.states.lock().unwrap().get(&account).cloned().unwrap();
    assert_eq!(marker.phase, UsageRefreshPhase::Idle);
    assert!(marker.terminal_result.is_none());
    assert!(marker.last_good.is_none());
    assert!(marker.terminal_error.is_none());
    assert_eq!(marker.generation, reset.generation);
    assert_eq!(marker.retry_deadline_epoch, Some(1_300));

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
    let coordinator = catalog_coordinator_with_fake_clock(
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

    let error = coordinator.reconcile_catalog([], 10_000).unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(store.states.lock().unwrap().get(&account).cloned(), before);
    assert_eq!(
        coordinator.current(&account, 10_000).unwrap().phase,
        UsageRefreshPhase::Completed
    );

    *store.purge_error.lock().unwrap() = None;
    coordinator.reconcile_catalog([], 10_001).unwrap();
    let revoked = coordinator.current(&account, 10_001).unwrap();
    assert_eq!(revoked.phase, UsageRefreshPhase::Failed);
    assert_eq!(
        revoked.error.unwrap().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
}

#[test]
fn same_capability_revision_change_fences_in_flight_join_immediately() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = Arc::new(catalog_coordinator_with_fake_clock(
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
    let first = coordinator_with_fake_clock(
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

    let second = catalog_coordinator_with_fake_clock(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<FileAccountStateStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "revision-a")],
    );
    second.reconcile_catalog([], 1_002).unwrap();
    let tombstone = store
        .load(&account, 1_002)
        .unwrap()
        .expect("revocation retains only the active account cooldown tombstone");
    assert_eq!(tombstone.phase, UsageRefreshPhase::Idle);
    assert!(tombstone.terminal_result.is_none());
    assert!(tombstone.last_good.is_none());
    assert!(tombstone.terminal_error.is_none());
    assert_eq!(tombstone.provider_invoked_at_epoch, Some(1_000));
    assert_eq!(tombstone.success_deadline_epoch, Some(1_300));
    drop(second);

    let restarted = catalog_coordinator_with_fake_clock(
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
