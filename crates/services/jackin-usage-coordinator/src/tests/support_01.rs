// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Default)]
pub(super) struct FakeMonotonicClock {
    elapsed_seconds: AtomicU64,
}

impl FakeMonotonicClock {
    pub(super) fn advance(&self, duration: Duration) {
        self.elapsed_seconds
            .fetch_add(duration.as_secs(), Ordering::SeqCst);
    }
}

impl MonotonicClock for FakeMonotonicClock {
    fn now(&self) -> Duration {
        Duration::from_secs(self.elapsed_seconds.load(Ordering::SeqCst))
    }

    fn sample(&self, fallback_epoch: i64) -> ClockSample {
        let fallback_seconds = u64::try_from(fallback_epoch.max(0)).unwrap_or(u64::MAX);
        let observed = self
            .elapsed_seconds
            .fetch_max(fallback_seconds, Ordering::SeqCst)
            .max(fallback_seconds);
        ClockSample::anchored(fallback_epoch, Duration::from_secs(observed))
    }
}

#[derive(Default)]
pub(super) struct ManualClock {
    wall_epoch_seconds: AtomicU64,
    monotonic_seconds: AtomicU64,
}

impl ManualClock {
    pub(super) fn at(epoch_seconds: u64) -> Self {
        Self {
            wall_epoch_seconds: AtomicU64::new(epoch_seconds),
            monotonic_seconds: AtomicU64::new(0),
        }
    }

    pub(super) fn advance(&self, duration: Duration) {
        self.wall_epoch_seconds
            .fetch_add(duration.as_secs(), Ordering::SeqCst);
        self.monotonic_seconds
            .fetch_add(duration.as_secs(), Ordering::SeqCst);
    }

    pub(super) fn set_wall_epoch(&self, epoch_seconds: u64) {
        self.wall_epoch_seconds
            .store(epoch_seconds, Ordering::SeqCst);
    }
}

impl MonotonicClock for ManualClock {
    fn now(&self) -> Duration {
        Duration::from_secs(self.monotonic_seconds.load(Ordering::SeqCst))
    }

    fn sample(&self, _fallback_epoch: i64) -> ClockSample {
        ClockSample::anchored(
            i64::try_from(self.wall_epoch_seconds.load(Ordering::SeqCst)).unwrap_or(i64::MAX),
            self.now(),
        )
    }
}

#[derive(Default)]
pub(super) struct MemoryStore {
    pub(super) states: Mutex<BTreeMap<UsageAccountCapability, AccountStateEnvelope>>,
    pub(super) purges: Mutex<Vec<UsageAccountCapability>>,
    pub(super) load_error: Mutex<Option<StateStoreError>>,
    store_error: Mutex<Option<StateStoreError>>,
    pub(super) purge_error: Mutex<Option<StateStoreError>>,
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

    fn purge(&self, capability: &UsageAccountCapability) -> Result<(), StateStoreError> {
        if let Some(error) = *self.purge_error.lock().unwrap() {
            return Err(error);
        }
        self.states.lock().unwrap().remove(capability);
        self.purges.lock().unwrap().push(capability.clone());
        Ok(())
    }
}

pub(super) struct GateExecutor {
    pub(super) calls: AtomicUsize,
    active: AtomicUsize,
    pub(super) max_active: AtomicUsize,
    started: (Mutex<usize>, Condvar),
    permits: (Mutex<usize>, Condvar),
    outcome: Mutex<ProviderProbeOutcome>,
}

impl GateExecutor {
    pub(super) fn new(outcome: ProviderProbeOutcome) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
            started: (Mutex::new(0), Condvar::new()),
            permits: (Mutex::new(0), Condvar::new()),
            outcome: Mutex::new(outcome),
        }
    }

    pub(super) fn wait_started(&self, expected: usize) {
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

    pub(super) fn release(&self, count: usize) {
        let (lock, changed) = &self.permits;
        *lock.lock().unwrap() += count;
        changed.notify_all();
    }

    pub(super) fn wait_idle(&self) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.active.load(Ordering::SeqCst) != 0 {
            assert!(Instant::now() < deadline, "provider probe did not finish");
            std::thread::yield_now();
        }
    }

    pub(super) fn set_outcome(&self, outcome: ProviderProbeOutcome) {
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
            let (next, wait) = permit_changed
                .wait_timeout(permits, Duration::from_secs(2))
                .unwrap();
            permits = next;
            assert!(!wait.timed_out(), "provider probe permit was not released");
        }
        *permits -= 1;
        self.active.fetch_sub(1, Ordering::SeqCst);
        self.outcome.lock().unwrap().clone()
    }
}

pub(super) fn capability(id: &str) -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: id.into(),
        surface_id: "claude".into(),
    }
}

pub(super) fn quota_view(epoch: i64, percent: u8) -> FocusedUsageView {
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

pub(super) fn coordinator(
    executor: Arc<GateExecutor>,
    store: Arc<MemoryStore>,
    config: UsageCoordinatorConfig,
) -> UsageCoordinator {
    let provider: Arc<dyn UsageProviderExecutor> = executor;
    let state_store: Arc<dyn AccountStateStore> = store;
    coordinator_with_fake_clock(provider, state_store, config)
}

pub(super) fn coordinator_with_fake_clock(
    executor: Arc<dyn UsageProviderExecutor>,
    store: Arc<dyn AccountStateStore>,
    config: UsageCoordinatorConfig,
) -> UsageCoordinator {
    let clock: Arc<dyn MonotonicClock> = Arc::new(FakeMonotonicClock::default());
    UsageCoordinator::start_with_clock(executor, store, config, None, None, clock)
}

pub(super) fn catalog_coordinator_with_fake_clock(
    executor: Arc<dyn UsageProviderExecutor>,
    store: Arc<dyn AccountStateStore>,
    config: UsageCoordinatorConfig,
    catalog: impl IntoIterator<Item = UsageCatalogEntry>,
) -> UsageCoordinator {
    let catalog = catalog
        .into_iter()
        .map(|entry| (entry.capability, entry.revision))
        .collect();
    let clock: Arc<dyn MonotonicClock> = Arc::new(FakeMonotonicClock::default());
    UsageCoordinator::start_with_clock(executor, store, config, Some(catalog), None, clock)
}

pub(super) fn join_ok(
    coordinator: &UsageCoordinator,
    capability: &UsageAccountCapability,
    generation: u64,
    now_epoch: i64,
) -> UsageGenerationView {
    coordinator
        .join_generation(capability, generation, Duration::from_secs(2), now_epoch)
        .unwrap()
}

pub(super) struct UpdatingFailureStore {
    inner: FileAccountStateStore,
    accounts: std::path::PathBuf,
    saved_accounts: std::path::PathBuf,
    failed_updates: AtomicUsize,
    persistent: bool,
}

impl UpdatingFailureStore {
    fn restore(&self) {
        std::fs::remove_file(&self.accounts).unwrap();
        std::fs::rename(&self.saved_accounts, &self.accounts).unwrap();
    }
}

impl AccountStateStore for UpdatingFailureStore {
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
        if envelope.phase == UsageRefreshPhase::Updating
            && self.failed_updates.fetch_add(1, Ordering::SeqCst) == 0
        {
            std::fs::rename(&self.accounts, &self.saved_accounts).unwrap();
            std::fs::write(&self.accounts, b"fixture blocks account directory").unwrap();
            let result = self.inner.store(envelope, now_epoch);
            assert_eq!(result, Err(StateStoreError::Unavailable));
            if !self.persistent {
                self.restore();
            }
            return result;
        }
        self.inner.store(envelope, now_epoch)
    }

    fn purge(&self, capability: &UsageAccountCapability) -> Result<(), StateStoreError> {
        self.inner.purge(capability)
    }
}

pub(super) fn assert_updating_store_failure_terminates(persistent: bool) {
    let (completed, completion) = mpsc::channel();
    // Own the coordinator inside this thread: a broken worker's Drop must not
    // keep the test harness waiting beyond the regression's bounded deadline.
    let fixture = std::thread::spawn(move || {
        let temp = tempfile::tempdir().unwrap();
        let accounts = temp.path().join("accounts");
        let store = Arc::new(UpdatingFailureStore {
            inner: FileAccountStateStore::at(accounts.clone()),
            accounts,
            saved_accounts: temp.path().join("saved-accounts"),
            failed_updates: AtomicUsize::new(0),
            persistent,
        });
        let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
            quota_view(1_000, 80),
        )));
        let account = capability("account-a");
        let coordinator = catalog_coordinator_with_fake_clock(
            Arc::<ImmediateExecutor>::clone(&executor),
            Arc::<UpdatingFailureStore>::clone(&store),
            UsageCoordinatorConfig::default(),
            [catalog_entry(&account, "revision-a")],
        );
        let queued = coordinator
            .request_refresh(&account, 0, true, 1_000)
            .unwrap();
        let terminal =
            coordinator.join_generation(&account, queued.generation, Duration::from_secs(2), 1_001);
        if persistent {
            assert_eq!(
                terminal.unwrap_err().kind,
                UsageCoordinationErrorKind::Unavailable
            );
            assert!(
                coordinator.is_idle(),
                "failed persistence must release ownership"
            );
            assert_eq!(
                coordinator.current(&account, 1_001).unwrap_err().kind,
                UsageCoordinationErrorKind::Unavailable
            );
            store.restore();
        } else {
            let terminal = terminal.unwrap();
            assert_eq!(terminal.generation, queued.generation);
            assert_eq!(terminal.phase, UsageRefreshPhase::Failed);
            assert_eq!(
                terminal.error.unwrap().kind,
                UsageCoordinationErrorKind::Unavailable
            );
        }
        let recovered = coordinator.current(&account, 1_002).unwrap();
        assert_eq!(recovered.phase, UsageRefreshPhase::Failed);
        assert_eq!(
            recovered.error.unwrap().kind,
            UsageCoordinationErrorKind::Unavailable
        );
        let durable = store.load(&account, 1_002).unwrap().unwrap();
        assert_eq!(durable.phase, UsageRefreshPhase::Failed);
        assert_eq!(durable.generation, queued.generation);
        assert_eq!(durable.consecutive_failures, 1);
        let joined = join_ok(&coordinator, &account, queued.generation, 1_002);
        assert_eq!(joined.phase, UsageRefreshPhase::Failed);
        assert_eq!(
            joined.error.unwrap().kind,
            UsageCoordinationErrorKind::Unavailable
        );
        assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
        assert_eq!(store.failed_updates.load(Ordering::SeqCst), 1);
        assert!(coordinator.is_idle());
        coordinator.reconcile_catalog([], 1_003).unwrap();
        let tombstone = store
            .load(&account, 1_003)
            .unwrap()
            .expect("catalog removal must retain the failed generation's retry fence");
        assert_eq!(tombstone.phase, UsageRefreshPhase::Idle);
        assert!(tombstone.terminal_result.is_none());
        assert!(tombstone.last_good.is_none());
        assert!(tombstone.terminal_error.is_none());
        assert_eq!(tombstone.provider_invoked_at_epoch, None);
        assert_eq!(tombstone.retry_deadline_epoch, Some(1_030));
        drop(coordinator);
        completed.send(()).unwrap();
    });
    completion
        .recv_timeout(Duration::from_secs(5))
        .expect("Updating store failure deadlocked terminal transition or worker shutdown");
    fixture.join().unwrap();
}

pub(super) fn catalog_entry(account: &UsageAccountCapability, revision: &str) -> UsageCatalogEntry {
    UsageCatalogEntry {
        capability: account.clone(),
        revision: revision.to_owned(),
    }
}

pub(super) struct CadenceMemoryStore {
    pub(super) states: Mutex<BTreeMap<UsageAccountCapability, AccountStateEnvelope>>,
}

impl AccountStateStore for CadenceMemoryStore {
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

pub(super) struct ImmediateExecutor {
    pub(super) calls: AtomicUsize,
    outcome: Mutex<ProviderProbeOutcome>,
}

impl ImmediateExecutor {
    pub(super) fn new(outcome: ProviderProbeOutcome) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            outcome: Mutex::new(outcome),
        }
    }

    pub(super) fn set_outcome(&self, outcome: ProviderProbeOutcome) {
        *self.outcome.lock().unwrap() = outcome;
    }
}

impl UsageProviderExecutor for ImmediateExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.outcome.lock().unwrap().clone()
    }
}

pub(super) struct CadenceGateExecutor {
    pub(super) calls: AtomicUsize,
    pub(super) started: (Mutex<usize>, Condvar),
    pub(super) permits: (Mutex<usize>, Condvar),
    pub(super) outcome: Mutex<ProviderProbeOutcome>,
}
