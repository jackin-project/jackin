// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicUsize, Ordering};

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};

use super::*;

#[derive(Default)]
struct MemoryStore {
    states: Mutex<BTreeMap<UsageAccountCapability, AccountStateEnvelope>>,
    load_error: Mutex<Option<StateStoreError>>,
    store_error: Mutex<Option<StateStoreError>>,
}

impl AccountStateStore for MemoryStore {
    fn load(
        &self,
        capability: &UsageAccountCapability,
        _now_epoch: i64,
    ) -> Result<Option<AccountStateEnvelope>, StateStoreError> {
        if let Some(error) = *self.load_error.lock().unwrap() {
            return Err(error);
        }
        Ok(self.states.lock().unwrap().get(capability).cloned())
    }

    fn store(
        &self,
        envelope: &AccountStateEnvelope,
        _now_epoch: i64,
    ) -> Result<(), StateStoreError> {
        if let Some(error) = *self.store_error.lock().unwrap() {
            return Err(error);
        }
        self.states
            .lock()
            .unwrap()
            .insert(envelope.capability.clone(), envelope.clone());
        Ok(())
    }
}

struct GateExecutor {
    calls: AtomicUsize,
    active: AtomicUsize,
    max_active: AtomicUsize,
    started: (Mutex<usize>, Condvar),
    permits: (Mutex<usize>, Condvar),
    outcome: Mutex<ProviderProbeOutcome>,
}

impl GateExecutor {
    fn new(outcome: ProviderProbeOutcome) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
            started: (Mutex::new(0), Condvar::new()),
            permits: (Mutex::new(0), Condvar::new()),
            outcome: Mutex::new(outcome),
        }
    }

    fn wait_started(&self, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(2);
        let (lock, changed) = &self.started;
        let mut started = lock.lock().unwrap();
        while *started < expected {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "provider probe did not start");
            let (next, wait) = changed.wait_timeout(started, remaining).unwrap();
            started = next;
            assert!(!wait.timed_out(), "provider probe did not start");
        }
    }

    fn release(&self, count: usize) {
        let (lock, changed) = &self.permits;
        *lock.lock().unwrap() += count;
        changed.notify_all();
    }

    fn set_outcome(&self, outcome: ProviderProbeOutcome) {
        *self.outcome.lock().unwrap() = outcome;
    }
}

impl UsageProviderExecutor for GateExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        let (started_lock, started_changed) = &self.started;
        *started_lock.lock().unwrap() += 1;
        started_changed.notify_all();

        let (permit_lock, permit_changed) = &self.permits;
        let mut permits = permit_lock.lock().unwrap();
        while *permits == 0 {
            permits = permit_changed.wait(permits).unwrap();
        }
        *permits -= 1;
        self.active.fetch_sub(1, Ordering::SeqCst);
        self.outcome.lock().unwrap().clone()
    }
}

fn capability(id: &str) -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: id.into(),
        surface_id: "claude".into(),
    }
}

fn quota_view(epoch: i64, percent: u8) -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("fixture", epoch);
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.account.provider_label = "Claude".into();
    view.account.account_label = "account@example.test".into();
    view.buckets = vec![QuotaBucketView {
        label: "Session".into(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(percent),
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

fn coordinator(
    executor: Arc<GateExecutor>,
    store: Arc<MemoryStore>,
    config: UsageCoordinatorConfig,
) -> UsageCoordinator {
    UsageCoordinator::new(executor, store, config)
}

fn join_ok(
    coordinator: &UsageCoordinator,
    capability: &UsageAccountCapability,
    generation: u64,
    now_epoch: i64,
) -> UsageGenerationView {
    coordinator
        .join_generation(capability, generation, Duration::from_secs(2), now_epoch)
        .unwrap()
}

#[test]
fn coordinator_winner_joiner_and_force_join_share_one_generation() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(MemoryStore::default());
    let coordinator = coordinator(
        Arc::clone(&executor),
        store,
        UsageCoordinatorConfig::default(),
    );
    let account = capability("account-a");

    let winner = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    let joiner = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    assert_eq!(winner.generation, 1);
    assert_eq!(joiner.generation, 1);
    assert!(joiner.phase.is_active());

    executor.release(1);
    let terminal = join_ok(&coordinator, &account, 1, 1_001);
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn coordinator_post_terminal_manual_refresh_starts_later_generation() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let account = capability("account-a");
    let first = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    executor.release(1);
    let first = join_ok(&coordinator, &account, first.generation, 1_001);

    let stale_click = coordinator
        .request_refresh(&account, 0, true, 1_002)
        .unwrap();
    assert_eq!(stale_click.generation, first.generation);
    let later_click = coordinator
        .request_refresh(&account, first.generation, true, 1_002)
        .unwrap();
    assert_eq!(later_click.generation, 2);
    executor.wait_started(2);
    executor.release(1);
    assert_eq!(
        join_ok(&coordinator, &account, 2, 1_003).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn coordinator_ambient_tick_honors_success_cooldown() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let account = capability("account-a");
    coordinator
        .request_refresh(&account, 0, false, 1_000)
        .unwrap();
    executor.wait_started(1);
    executor.release(1);
    let terminal = join_ok(&coordinator, &account, 1, 1_001);
    let ambient = coordinator
        .request_refresh(&account, terminal.generation, false, 1_002)
        .unwrap();
    assert_eq!(ambient.generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn coordinator_refresh_all_deduplicates_canonical_accounts() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let account_a = capability("account-a");
    let account_b = capability("account-b");
    let results = coordinator.request_refresh_all(
        [
            (account_a.clone(), 0),
            (account_a.clone(), 0),
            (account_b.clone(), 0),
        ],
        true,
        1_000,
    );
    assert_eq!(results.len(), 2);
    executor.wait_started(2);
    executor.release(2);
    drop(join_ok(&coordinator, &account_a, 1, 1_001));
    drop(join_ok(&coordinator, &account_b, 1, 1_001));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn coordinator_empty_result_is_failure_and_preserves_last_good() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let account = capability("account-a");
    coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    executor.release(1);
    drop(join_ok(&coordinator, &account, 1, 1_001));

    executor.set_outcome(ProviderProbeOutcome::success(
        FocusedUsageView::unavailable("empty", 1_002),
    ));
    coordinator
        .request_refresh(&account, 1, true, 1_002)
        .unwrap();
    executor.wait_started(2);
    executor.release(1);
    let terminal = join_ok(&coordinator, &account, 2, 1_003);
    assert_eq!(terminal.phase, UsageRefreshPhase::Failed);
    assert_eq!(
        terminal.snapshot.unwrap().buckets[0].remaining_percent,
        Some(80)
    );
}

#[test]
fn coordinator_unavailable_or_corrupt_state_makes_zero_provider_calls() {
    for error in [StateStoreError::Unavailable, StateStoreError::Corrupt] {
        let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
            quota_view(1_000, 80),
        )));
        let store = Arc::new(MemoryStore::default());
        *store.load_error.lock().unwrap() = Some(error);
        let coordinator = coordinator(
            Arc::clone(&executor),
            store,
            UsageCoordinatorConfig::default(),
        );
        let result = coordinator.request_refresh(&capability("account-a"), 0, true, 1_000);
        result.unwrap_err();
        assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn coordinator_capsule_capability_rejects_non_forwarded_account() {
    let account_a = capability("account-a");
    let account_b = capability("account-b");
    let allowlist = UsageCapabilitySet::new([account_a.clone()]);
    allowlist.authorize(&account_a).unwrap();
    let error = allowlist.authorize(&account_b).unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
}

#[test]
fn coordinator_capsule_surface_rejects_ambiguous_accounts() {
    let account_a = capability("account-a");
    let account_b = capability("account-b");
    let allowlist = UsageCapabilitySet::new([account_a, account_b]);

    let error = allowlist.resolve_surface("claude").unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
}

#[test]
fn coordinator_failure_shares_retry_deadline_and_last_good() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let account = capability("account-a");
    coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    executor.release(1);
    drop(join_ok(&coordinator, &account, 1, 1_001));
    executor.set_outcome(ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::RateLimited,
        message: "provider rate limited".into(),
        retry_at_epoch: Some(2_000),
    });
    coordinator
        .request_refresh(&account, 1, true, 1_002)
        .unwrap();
    executor.wait_started(2);
    executor.release(1);
    let failed = join_ok(&coordinator, &account, 2, 1_003);
    assert_eq!(failed.retry_at_epoch, Some(2_000));
    assert!(failed.snapshot.is_some());
    let suppressed = coordinator
        .request_refresh(&account, 2, true, 1_004)
        .unwrap();
    assert_eq!(suppressed.generation, 2);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
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
        Arc::clone(&executor),
        Arc::clone(&store),
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
        Arc::clone(&executor),
        Arc::clone(&store),
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
fn coordinator_unknown_bootstrap_serializes_per_provider() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let bootstrap = capability("bootstrap-claude");
    coordinator
        .request_refresh(&bootstrap, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    let joined = coordinator
        .request_refresh(&bootstrap, 0, true, 1_000)
        .unwrap();
    assert_eq!(joined.generation, 1);
    executor.release(1);
    drop(join_ok(&coordinator, &bootstrap, 1, 1_001));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn coordinator_distinct_accounts_refresh_within_concurrency_bound() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let config = UsageCoordinatorConfig {
        max_concurrency: 2,
        ..UsageCoordinatorConfig::default()
    };
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        config,
    );
    let account_a = capability("account-a");
    let account_b = capability("account-b");
    coordinator
        .request_refresh(&account_a, 0, true, 1_000)
        .unwrap();
    coordinator
        .request_refresh(&account_b, 0, true, 1_000)
        .unwrap();
    executor.wait_started(2);
    assert_eq!(executor.max_active.load(Ordering::SeqCst), 2);
    executor.release(2);
    drop(join_ok(&coordinator, &account_a, 1, 1_001));
    drop(join_ok(&coordinator, &account_b, 1, 1_001));
}
