// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::PermissionsExt as _;
use std::sync::atomic::{AtomicUsize, Ordering};

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};

use super::policy::UsagePolicy;
use super::state::PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION;
use super::*;

#[derive(Default)]
struct FakeMonotonicClock {
    state: Mutex<FakeClockState>,
}

#[derive(Default)]
struct FakeClockState {
    monotonic: Duration,
    wall_epoch: Option<Duration>,
}

impl FakeMonotonicClock {
    fn with_wall_epoch(wall_epoch: Duration) -> Self {
        Self {
            state: Mutex::new(FakeClockState {
                monotonic: Duration::ZERO,
                wall_epoch: Some(wall_epoch),
            }),
        }
    }

    fn advance(&self, duration: Duration) {
        let mut state = self.state.lock().unwrap();
        state.monotonic = state.monotonic.saturating_add(duration);
        if let Some(wall_epoch) = &mut state.wall_epoch {
            *wall_epoch = wall_epoch.saturating_add(duration);
        }
    }

    fn jump_wall_forward(&self, duration: Duration) {
        let mut state = self.state.lock().unwrap();
        if let Some(wall_epoch) = &mut state.wall_epoch {
            *wall_epoch = wall_epoch.saturating_add(duration);
        }
    }
}

impl MonotonicClock for FakeMonotonicClock {
    fn now(&self) -> Duration {
        self.state.lock().unwrap().monotonic
    }

    fn sample(&self, fallback_epoch: i64) -> ClockSample {
        let mut state = self.state.lock().unwrap();
        let initial_wall_epoch = ClockSample::anchored(fallback_epoch, state.monotonic).wall_epoch;
        let wall_epoch = *state.wall_epoch.get_or_insert(initial_wall_epoch);
        ClockSample {
            wall_epoch,
            monotonic: state.monotonic,
        }
    }
}

#[derive(Default)]
struct MemoryStore {
    states: Mutex<BTreeMap<UsageAccountCapability, AccountStateEnvelope>>,
    purges: Mutex<Vec<UsageAccountCapability>>,
    load_error: Mutex<Option<StateStoreError>>,
    store_error: Mutex<Option<StateStoreError>>,
    purge_error: Mutex<Option<StateStoreError>>,
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

struct AdvancingUpdatingStore {
    inner: Arc<MemoryStore>,
    clock: Arc<FakeMonotonicClock>,
    delay_once: Mutex<Option<Duration>>,
}

impl AccountStateStore for AdvancingUpdatingStore {
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
            && let Some(delay) = self.delay_once.lock().unwrap().take()
        {
            self.clock.advance(delay);
        }
        self.inner.store(envelope, now_epoch)
    }

    fn purge(&self, capability: &UsageAccountCapability) -> Result<(), StateStoreError> {
        self.inner.purge(capability)
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

    fn wait_idle(&self) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.active.load(Ordering::SeqCst) != 0 {
            assert!(Instant::now() < deadline, "provider probe did not finish");
            std::thread::yield_now();
        }
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

fn completed_claude_attempt(
    account: &UsageAccountCapability,
    invoked_at_epoch: i64,
    success_deadline_epoch: i64,
) -> AccountStateEnvelope {
    let completed_at_epoch = invoked_at_epoch.saturating_add(1);
    let view = quota_view(completed_at_epoch, 80);
    let mut envelope = AccountStateEnvelope::idle(account.clone());
    envelope.generation = 1;
    envelope.phase = UsageRefreshPhase::Completed;
    envelope.terminal_result = Some(view.clone());
    envelope.last_good = Some(view);
    envelope.started_at_epoch = Some(invoked_at_epoch);
    envelope.provider_invoked_at_epoch = Some(invoked_at_epoch);
    envelope.completed_at_epoch = Some(completed_at_epoch);
    envelope.success_deadline_epoch = Some(success_deadline_epoch);
    envelope
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

fn wait_for_idle_cooldown_tombstone(
    store: &dyn AccountStateStore,
    capability: &UsageAccountCapability,
    retry_deadline_epoch: i64,
    now_epoch: i64,
) -> AccountStateEnvelope {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(envelope) = store.load(capability, now_epoch).unwrap()
            && envelope.phase == UsageRefreshPhase::Idle
            && envelope.retry_deadline_epoch == Some(retry_deadline_epoch)
        {
            return envelope;
        }
        assert!(
            Instant::now() < deadline,
            "cooldown tombstone did not finish"
        );
        std::thread::yield_now();
    }
}

struct UpdatingFailureStore {
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

fn assert_updating_store_failure_terminates(persistent: bool) {
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
        let coordinator = UsageCoordinator::with_catalog(
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
        let tombstone = store.load(&account, 1_003).unwrap().unwrap();
        assert_eq!(tombstone.phase, UsageRefreshPhase::Idle);
        assert!(tombstone.terminal_result.is_none());
        assert!(tombstone.last_good.is_none());
        assert!(tombstone.retry_deadline_epoch.is_some());
        drop(coordinator);
        completed.send(()).unwrap();
    });
    completion
        .recv_timeout(Duration::from_secs(5))
        .expect("Updating store failure deadlocked terminal transition or worker shutdown");
    fixture.join().unwrap();
}

fn catalog_entry(account: &UsageAccountCapability, revision: &str) -> UsageCatalogEntry {
    UsageCatalogEntry {
        capability: account.clone(),
        revision: revision.to_owned(),
    }
}

struct CadenceMemoryStore {
    states: Mutex<BTreeMap<UsageAccountCapability, AccountStateEnvelope>>,
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

struct ImmediateExecutor {
    calls: AtomicUsize,
    outcome: Mutex<ProviderProbeOutcome>,
}

impl ImmediateExecutor {
    fn new(outcome: ProviderProbeOutcome) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            outcome: Mutex::new(outcome),
        }
    }

    fn set_outcome(&self, outcome: ProviderProbeOutcome) {
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

struct ClockRecordingExecutor {
    clock: Arc<FakeMonotonicClock>,
    calls: AtomicUsize,
    starts: Mutex<Vec<ClockSample>>,
    outcome: ProviderProbeOutcome,
}

impl UsageProviderExecutor for ClockRecordingExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.starts.lock().unwrap().push(self.clock.sample(0));
        self.outcome.clone()
    }
}

struct CadenceGateExecutor {
    calls: AtomicUsize,
    started: (Mutex<usize>, Condvar),
    permits: (Mutex<usize>, Condvar),
    outcome: Mutex<ProviderProbeOutcome>,
}

impl CadenceGateExecutor {
    fn new(outcome: ProviderProbeOutcome) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            started: (Mutex::new(0), Condvar::new()),
            permits: (Mutex::new(0), Condvar::new()),
            outcome: Mutex::new(outcome),
        }
    }

    fn wait_started(&self) {
        let deadline = Instant::now() + Duration::from_secs(2);
        let (lock, changed) = &self.started;
        let mut started = lock.lock().unwrap();
        while *started < 1 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "provider probe did not start");
            let (next, wait) = changed.wait_timeout(started, remaining).unwrap();
            started = next;
            assert!(!wait.timed_out(), "provider probe did not start");
        }
    }

    fn release(&self) {
        let (lock, changed) = &self.permits;
        *lock.lock().unwrap() += 1;
        changed.notify_all();
    }
}

impl UsageProviderExecutor for CadenceGateExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let (started_lock, started_changed) = &self.started;
        *started_lock.lock().unwrap() += 1;
        started_changed.notify_all();
        let (permit_lock, permit_changed) = &self.permits;
        let mut permits = permit_lock.lock().unwrap();
        while *permits == 0 {
            permits = permit_changed.wait(permits).unwrap();
        }
        *permits -= 1;
        self.outcome.lock().unwrap().clone()
    }
}

fn cadence_quota_view(epoch: i64) -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("fixture", epoch);
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.buckets = vec![QuotaBucketView {
        label: "Session".into(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(80),
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

fn cadence_coordinator<E>(executor: Arc<E>, config: UsageCoordinatorConfig) -> UsageCoordinator
where
    E: UsageProviderExecutor + 'static,
{
    UsageCoordinator::new(
        executor,
        Arc::new(CadenceMemoryStore {
            states: Mutex::new(BTreeMap::new()),
        }),
        config,
    )
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
fn coordinator_revisioned_capabilities_do_not_join_in_flight_probe() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let old_catalog_capability = capability("account-a:catalog-old");
    let current_catalog_capability = capability("account-a:catalog-current");

    let old = coordinator
        .request_refresh(&old_catalog_capability, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    let current = coordinator
        .request_refresh(&current_catalog_capability, 0, true, 1_000)
        .unwrap();
    executor.wait_started(2);

    assert_eq!(old.generation, 1);
    assert_eq!(current.generation, 1);
    executor.release(2);
    assert_eq!(
        join_ok(&coordinator, &old_catalog_capability, 1, 1_001).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(
        join_ok(&coordinator, &current_catalog_capability, 1, 1_001).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn coordinator_post_terminal_manual_refresh_obeys_claude_minimum_interval() {
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
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let later_click = coordinator
        .request_refresh(&account, first.generation, true, 1_400)
        .unwrap();
    assert_eq!(later_click.generation, 2);
    executor.wait_started(2);
    executor.release(1);
    assert_eq!(
        join_ok(&coordinator, &account, 2, 1_401).phase,
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
        FocusedUsageView::unavailable("empty", 1_400),
    ));
    coordinator
        .request_refresh(&account, 1, true, 1_400)
        .unwrap();
    executor.wait_started(2);
    executor.release(1);
    let terminal = join_ok(&coordinator, &account, 2, 1_401);
    assert_eq!(terminal.phase, UsageRefreshPhase::Failed);
    assert_eq!(
        terminal.snapshot.unwrap().buckets[0].remaining_percent,
        Some(80)
    );
}

#[test]
fn coordinator_stale_and_error_success_results_schedule_retry() {
    for status in [UsageSnapshotStatus::Stale, UsageSnapshotStatus::Error] {
        let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
            quota_view(1_000, 80),
        )));
        #[expect(
            clippy::clone_on_ref_ptr,
            reason = "coerce concrete executor to shared trait object"
        )]
        let provider_executor: Arc<dyn UsageProviderExecutor> = executor.clone();
        let coordinator = UsageCoordinator::new(
            provider_executor,
            Arc::new(MemoryStore::default()),
            UsageCoordinatorConfig::default(),
        );
        let account = capability("account-a");
        let first = coordinator
            .request_refresh(&account, 0, true, 1_000)
            .unwrap();
        let first = join_ok(&coordinator, &account, first.generation, 1_001);

        let mut failed_view = quota_view(1_002, 80);
        failed_view.status = status;
        failed_view.last_error = Some("provider result is not current".to_owned());
        executor.set_outcome(ProviderProbeOutcome::success(failed_view));
        let second = coordinator
            .request_refresh(&account, first.generation, true, 1_400)
            .unwrap();
        let failed = join_ok(&coordinator, &account, second.generation, 1_401);

        assert_eq!(failed.phase, UsageRefreshPhase::Failed);
        assert_eq!(
            failed.error.as_ref().map(|error| error.kind),
            Some(UsageCoordinationErrorKind::ProviderUnavailable)
        );
        assert!(failed.retry_at_epoch.is_some());
        assert_eq!(
            failed
                .snapshot
                .as_ref()
                .and_then(|view| view.buckets.first())
                .and_then(|bucket| bucket.remaining_percent),
            Some(80)
        );
    }
}

#[test]
fn coordinator_unsupported_result_stays_unsupported_without_quota() {
    let mut view = quota_view(1_000, 80);
    view.status = UsageSnapshotStatus::Unsupported;
    view.buckets.clear();
    view.source = UsageSource::None;
    view.confidence = UsageConfidence::None;
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(view)));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to shared trait object"
    )]
    let provider_executor: Arc<dyn UsageProviderExecutor> = executor.clone();
    let coordinator = UsageCoordinator::new(
        provider_executor,
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let account = capability("unsupported-account");
    let queued = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    let result = join_ok(&coordinator, &account, queued.generation, 1_001);

    assert_eq!(result.phase, UsageRefreshPhase::Completed);
    assert_eq!(result.retry_at_epoch, None);
    let snapshot = result.snapshot.expect("unsupported snapshot");
    assert_eq!(snapshot.status, UsageSnapshotStatus::Unsupported);
    assert!(snapshot.buckets.is_empty());
}

#[test]
fn generation_view_exposes_the_latest_account_retry_deadline() {
    let account = capability("account-a");
    let mut envelope = AccountStateEnvelope::idle(account);
    envelope.rate_limit_deadline_epoch = Some(2_000);
    envelope.retry_deadline_epoch = Some(3_000);
    assert_eq!(generation_view(&envelope).retry_at_epoch, Some(3_000));

    envelope.rate_limit_deadline_epoch = Some(4_000);
    envelope.retry_deadline_epoch = Some(3_000);
    assert_eq!(generation_view(&envelope).retry_at_epoch, Some(4_000));
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
        .request_refresh(&account, 1, true, 1_400)
        .unwrap();
    executor.wait_started(2);
    executor.release(1);
    let failed = join_ok(&coordinator, &account, 2, 1_401);
    assert_eq!(failed.retry_at_epoch, Some(2_000));
    assert!(failed.snapshot.is_some());
    let suppressed = coordinator
        .request_refresh(&account, 2, true, 1_404)
        .unwrap();
    assert_eq!(suppressed.generation, 2);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
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
        let restart_epoch = attempt_floor - 1;
        let restart_clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
            u64::try_from(restart_epoch).unwrap(),
        )));
        #[expect(
            clippy::clone_on_ref_ptr,
            reason = "coerce concrete executor to shared trait object"
        )]
        let provider_executor: Arc<dyn UsageProviderExecutor> = restarted_executor.clone();
        #[expect(
            clippy::clone_on_ref_ptr,
            reason = "coerce paired fake clock to the coordinator clock port"
        )]
        let restart_clock_port: Arc<dyn MonotonicClock> = restart_clock.clone();
        let restarted = UsageCoordinator::start_with_clock(
            provider_executor,
            Arc::<FileAccountStateStore>::clone(&store),
            UsageCoordinatorConfig {
                success_cooldown: Duration::from_secs(1),
                ..UsageCoordinatorConfig::default()
            },
            None,
            None,
            restart_clock_port,
        );
        assert_eq!(
            restarted
                .current(&account, restart_epoch)
                .unwrap()
                .generation,
            1
        );
        assert_eq!(
            restarted.next_due_epoch(),
            Some(restart_epoch.saturating_add(300))
        );
        assert!(restarted.poll_due(restart_epoch).is_empty());
        let forced_early = restarted
            .request_refresh(&account, 1, true, restart_epoch)
            .unwrap();
        assert_eq!(forced_early.generation, 1);
        assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);

        restart_clock.advance(Duration::from_secs(299));
        let before_recovery_floor = restarted
            .request_refresh(&account, 1, true, restart_epoch + 299)
            .unwrap();
        assert_eq!(before_recovery_floor.generation, 1);
        assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);
        restart_clock.advance(Duration::from_secs(1));
        let recovery_deadline_epoch = restart_epoch + 300;
        let allowed = restarted
            .request_refresh(&account, 1, true, recovery_deadline_epoch)
            .unwrap();
        assert_eq!(allowed.generation, 2);
        assert_eq!(
            join_ok(&restarted, &account, 2, recovery_deadline_epoch + 1).phase,
            UsageRefreshPhase::Completed
        );
        assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn claude_rate_limit_circuit_uses_consecutive_failures_and_survives_restart() {
    let account = capability("repeated-rate-limit");
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(FileAccountStateStore::at(temp.path().join("accounts")));
    let clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        1_000,
    )));
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::Unauthorized,
        message: "first generation had an authorization failure".into(),
        retry_at_epoch: None,
    }));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to the coordinator port"
    )]
    let provider_executor: Arc<dyn UsageProviderExecutor> = executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "share the fake clock with the coordinator"
    )]
    let clock_port: Arc<dyn MonotonicClock> = clock.clone();
    let coordinator = UsageCoordinator::start_with_clock(
        provider_executor,
        Arc::<FileAccountStateStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        None,
        None,
        clock_port,
    );

    let first = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    let first_terminal = join_ok(&coordinator, &account, first.generation, 1_000);
    assert_eq!(first_terminal.phase, UsageRefreshPhase::Failed);
    assert_eq!(first_terminal.retry_at_epoch, Some(1_300));
    assert_eq!(
        store
            .load(&account, 1_000)
            .unwrap()
            .unwrap()
            .consecutive_failures,
        1
    );

    clock.advance(Duration::from_mins(5));
    executor.set_outcome(ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::RateLimited,
        message: "second generation was rate limited".into(),
        retry_at_epoch: Some(1_700),
    });
    let second = coordinator
        .request_refresh(&account, 1, true, 1_300)
        .unwrap();
    let second_terminal = join_ok(&coordinator, &account, second.generation, 1_300);
    assert_eq!(second_terminal.phase, UsageRefreshPhase::Failed);
    assert_eq!(second_terminal.retry_at_epoch, Some(1_700));
    let second_durable = store.load(&account, 1_300).unwrap().unwrap();
    assert_eq!(second_durable.consecutive_failures, 2);
    assert_eq!(second_durable.rate_limit_deadline_epoch, Some(1_700));

    clock.advance(Duration::from_secs(400));
    executor.set_outcome(ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::RateLimited,
        message: "third generation was rate limited".into(),
        retry_at_epoch: Some(1_750),
    });
    let third = coordinator
        .request_refresh(&account, 2, true, 1_700)
        .unwrap();
    let third_terminal = join_ok(&coordinator, &account, third.generation, 1_700);
    assert_eq!(third_terminal.phase, UsageRefreshPhase::Failed);
    assert_eq!(third_terminal.retry_at_epoch, Some(5_300));
    let durable = store.load(&account, 1_700).unwrap().unwrap();
    assert_eq!(durable.consecutive_failures, 3);
    assert_eq!(durable.provider_invoked_at_epoch, Some(1_700));
    assert_eq!(durable.retry_deadline_epoch, Some(5_300));
    assert_eq!(durable.rate_limit_deadline_epoch, Some(5_300));
    drop(coordinator);

    let restart_clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        5_000,
    )));
    let restart_executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(5_300, 70),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to the coordinator port"
    )]
    let restart_provider: Arc<dyn UsageProviderExecutor> = restart_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "share the restart fake clock with the coordinator"
    )]
    let restart_clock_port: Arc<dyn MonotonicClock> = restart_clock.clone();
    let restarted = UsageCoordinator::start_with_clock(
        restart_provider,
        Arc::<FileAccountStateStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        None,
        None,
        restart_clock_port,
    );
    let restored = restarted.current(&account, 5_000).unwrap();
    assert_eq!(restored.generation, 3);
    assert_eq!(restored.retry_at_epoch, Some(5_300));
    assert!(restarted.poll_due(5_000).is_empty());
    let forced_early = restarted.request_refresh(&account, 3, true, 5_000).unwrap();
    assert_eq!(forced_early.generation, 3);
    assert_eq!(restart_executor.calls.load(Ordering::SeqCst), 0);

    restart_clock.advance(Duration::from_secs(299));
    assert!(restarted.poll_due(5_299).is_empty());
    let still_early = restarted.request_refresh(&account, 3, true, 5_299).unwrap();
    assert_eq!(still_early.generation, 3);
    assert_eq!(restart_executor.calls.load(Ordering::SeqCst), 0);
    restart_clock.advance(Duration::from_secs(1));
    let half_open = restarted.poll_due(5_300);
    assert_eq!(half_open.len(), 1);
    assert_eq!(half_open[0].generation, 4);
    restart_executor.wait_started(1);
    let joined = restarted.request_refresh(&account, 3, true, 5_300).unwrap();
    assert_eq!(joined.generation, 4);
    assert_eq!(restart_executor.calls.load(Ordering::SeqCst), 1);
    restart_executor.release(1);
    let completed = join_ok(&restarted, &account, 4, 5_301);
    assert_eq!(completed.phase, UsageRefreshPhase::Completed);
    assert_eq!(restart_executor.calls.load(Ordering::SeqCst), 1);
    let recovered = store.load(&account, 5_301).unwrap().unwrap();
    assert_eq!(recovered.consecutive_failures, 0);
    assert_eq!(recovered.retry_deadline_epoch, None);
    assert_eq!(recovered.rate_limit_deadline_epoch, None);
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

    clock.advance(Duration::from_secs(299));
    let forced_early = coordinator
        .request_refresh(&account, 1, true, first_invocation + 299)
        .unwrap();
    assert_eq!(forced_early.generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    clock.advance(Duration::from_secs(1));
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
fn claude_attempt_spacing_uses_completion_pair_and_monotonic_gate() {
    let clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::new(
        1_000,
        900_000_000,
    )));
    let executor = Arc::new(ClockRecordingExecutor {
        clock: Arc::clone(&clock),
        calls: AtomicUsize::new(0),
        starts: Mutex::new(Vec::new()),
        outcome: ProviderProbeOutcome::success(quota_view(1_000, 80)),
    });
    let memory = Arc::new(MemoryStore::default());
    let store = Arc::new(AdvancingUpdatingStore {
        inner: Arc::clone(&memory),
        clock: Arc::clone(&clock),
        delay_once: Mutex::new(Some(Duration::from_millis(1_200))),
    });
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce the paired fake clock to the coordinator clock port"
    )]
    let clock_port: Arc<dyn MonotonicClock> = clock.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    let coordinator = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig::default(),
        None,
        None,
        clock_port,
    );
    let account = capability("fractional-completion");

    let first = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    let terminal = join_ok(&coordinator, &account, first.generation, 1_003);
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);

    let starts = executor.starts.lock().unwrap();
    assert_eq!(starts.len(), 1);
    let first_start = starts[0];
    assert_eq!(first_start.wall_epoch, Duration::new(1_002, 100_000_000));
    drop(starts);
    let envelope = memory.states.lock().unwrap().get(&account).unwrap().clone();
    assert_eq!(envelope.provider_invoked_at_epoch, Some(1_001));
    assert_eq!(envelope.success_deadline_epoch, Some(1_303));

    // A force call 299.9 seconds after the first executor start is still
    // blocked. Jumping wall time forward cannot bypass the paired monotonic
    // deadline either.
    clock.advance(Duration::from_millis(299_900));
    let early = coordinator
        .request_refresh(&account, 1, true, 1_302)
        .unwrap();
    assert_eq!(early.generation, 1);
    clock.jump_wall_forward(Duration::from_hours(1));
    let wall_jump = coordinator
        .request_refresh(&account, 1, true, 4_902)
        .unwrap();
    assert_eq!(wall_jump.generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    clock.advance(Duration::from_secs(1));
    let second = coordinator
        .request_refresh(&account, 1, true, 4_903)
        .unwrap();
    assert_eq!(second.generation, 2);
    let terminal = join_ok(&coordinator, &account, second.generation, 4_904);
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
    let starts = executor.starts.lock().unwrap();
    let elapsed = starts[1].monotonic.saturating_sub(first_start.monotonic);
    assert!(elapsed >= Duration::from_mins(5));
}

#[test]
fn claude_restart_after_forward_wall_jump_uses_fresh_monotonic_attempt_floor() {
    let account = capability("restart-forward-wall-jump");
    let store = Arc::new(MemoryStore::default());
    store
        .store(&completed_claude_attempt(&account, 1_000, 1_300), 1_001)
        .unwrap();
    let clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        5_000,
    )));
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(5_300, 79),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
    )]
    let clock_port: Arc<dyn MonotonicClock> = clock.clone();
    let coordinator = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig::default(),
        None,
        None,
        clock_port,
    );

    assert_eq!(
        coordinator.next_due_epoch_for_capabilities([account.clone()], 5_000),
        Some(5_300)
    );
    assert!(
        coordinator
            .poll_due_for_capabilities([account.clone()], 5_000)
            .is_empty()
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    coordinator
        .set_activity(&account, UsageActivity::DirectInteraction, false, 5_000)
        .unwrap();
    assert_eq!(
        coordinator.next_due_epoch_for_capabilities([account.clone()], 5_000),
        Some(5_300)
    );

    let immediate = coordinator
        .request_refresh(&account, 1, true, 5_000)
        .unwrap();
    assert_eq!(immediate.generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    clock.advance(Duration::from_secs(1));
    assert_eq!(coordinator.note_wake(5_001), 1);
    assert_eq!(
        coordinator.next_due_epoch_for_capabilities([account.clone()], 5_001),
        Some(5_300)
    );

    clock.advance(Duration::from_secs(298));
    let before_floor = coordinator
        .request_refresh(&account, 1, true, 5_299)
        .unwrap();
    assert_eq!(before_floor.generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    assert_eq!(
        coordinator.next_due_epoch_for_capabilities([account.clone()], 5_299),
        Some(5_300)
    );
    clock.jump_wall_forward(Duration::from_hours(1));
    assert_eq!(
        coordinator.next_due_epoch_for_capabilities([account.clone()], 8_899),
        Some(8_900)
    );
    assert!(
        coordinator
            .poll_due_for_capabilities([account.clone()], 8_899)
            .is_empty()
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    clock.advance(Duration::from_secs(1));
    let due = coordinator.poll_due_for_capabilities([account.clone()], 8_900);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].generation, 2);
    assert_eq!(
        join_ok(&coordinator, &account, 2, 8_901).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn claude_recovery_floor_does_not_shorten_later_persisted_retry_after() {
    let account = capability("restart-later-retry-after");
    let store = Arc::new(MemoryStore::default());
    let mut failed = completed_claude_attempt(&account, 900, 1_300);
    failed.phase = UsageRefreshPhase::Failed;
    failed.terminal_result = None;
    failed.terminal_error = Some(coordination_error(
        UsageCoordinationErrorKind::RateLimited,
        "provider rate limited",
    ));
    failed.rate_limit_deadline_epoch = Some(1_700);
    failed.retry_deadline_epoch = Some(1_700);
    store.store(&failed, 1_000).unwrap();
    let clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        1_000,
    )));
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_700, 79),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
    )]
    let clock_port: Arc<dyn MonotonicClock> = clock.clone();
    let coordinator = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig::default(),
        None,
        None,
        clock_port,
    );

    let initial = coordinator
        .request_refresh(&account, 1, true, 1_000)
        .unwrap();
    assert_eq!(initial.generation, 1);
    clock.advance(Duration::from_mins(5));
    let floor_passed = coordinator
        .request_refresh(&account, 1, true, 1_300)
        .unwrap();
    assert_eq!(floor_passed.generation, 1);
    clock.advance(Duration::from_secs(399));
    let retry_still_active = coordinator
        .request_refresh(&account, 1, true, 1_699)
        .unwrap();
    assert_eq!(retry_still_active.generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    clock.advance(Duration::from_secs(1));
    let retry_expired = coordinator
        .request_refresh(&account, 1, true, 1_700)
        .unwrap();
    assert_eq!(retry_expired.generation, 2);
    assert_eq!(
        join_ok(&coordinator, &account, 2, 1_701).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn claude_restart_without_invocation_evidence_has_no_recovery_floor() {
    let account = capability("restart-no-attempt");
    let store = Arc::new(MemoryStore::default());
    store
        .store(&AccountStateEnvelope::idle(account.clone()), 5_000)
        .unwrap();
    let clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        5_000,
    )));
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(5_000, 79),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
    )]
    let clock_port: Arc<dyn MonotonicClock> = clock.clone();
    let coordinator = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig::default(),
        None,
        None,
        clock_port,
    );

    let admitted = coordinator
        .request_refresh(&account, 0, true, 5_000)
        .unwrap();
    assert_eq!(admitted.generation, 1);
    assert_eq!(
        join_ok(&coordinator, &account, 1, 5_001).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn catalog_revoke_and_readd_preserves_recovery_floor_across_wall_jump() {
    let account = capability("readd-keeps-recovery-floor");
    let store = Arc::new(MemoryStore::default());
    store
        .store(&completed_claude_attempt(&account, 1_000, 1_300), 1_001)
        .unwrap();
    let clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        5_000,
    )));
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(8_900, 79),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
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
    assert_eq!(
        coordinator.current(&account, 5_000).unwrap().phase,
        UsageRefreshPhase::Completed
    );

    clock.jump_wall_forward(Duration::from_hours(1));
    coordinator.reconcile_catalog([], 8_600).unwrap();
    coordinator
        .reconcile_catalog([catalog_entry(&account, "revision-b")], 8_600)
        .unwrap();
    let readded = coordinator.current(&account, 8_600).unwrap();
    assert_eq!(readded.phase, UsageRefreshPhase::Idle);

    let before_floor = coordinator
        .request_refresh(&account, readded.generation, true, 8_600)
        .unwrap();
    assert_eq!(before_floor.generation, readded.generation);
    clock.advance(Duration::from_secs(299));
    let still_before_floor = coordinator
        .request_refresh(&account, readded.generation, true, 8_899)
        .unwrap();
    assert_eq!(still_before_floor.generation, readded.generation);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    clock.advance(Duration::from_secs(1));
    let after_floor = coordinator
        .request_refresh(&account, readded.generation, true, 8_900)
        .unwrap();
    assert_eq!(after_floor.generation, readded.generation + 1);
    assert_eq!(
        join_ok(&coordinator, &account, after_floor.generation, 8_901).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
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
    assert_eq!(durable.started_at_epoch, None);
    assert_eq!(durable.provider_invoked_at_epoch, Some(1_000));
    assert_eq!(durable.rate_limit_deadline_epoch, Some(5_000));
    assert_eq!(durable.retry_deadline_epoch, Some(5_000));
    drop(coordinator);

    let restarted_executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(5_000, 75),
    )));
    let restart_clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        2_000,
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to shared trait object"
    )]
    let provider_executor: Arc<dyn UsageProviderExecutor> = restarted_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
    )]
    let restart_clock_port: Arc<dyn MonotonicClock> = restart_clock.clone();
    let restarted = UsageCoordinator::start_with_clock(
        provider_executor,
        store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(
            account.clone(),
            "credential-revision-b".into(),
        )])),
        None,
        restart_clock_port,
    );
    let restored = restarted.current(&account, 2_000).unwrap();
    assert_eq!(restored.generation, 2);
    assert_eq!(restored.retry_at_epoch, Some(5_000));
    assert_eq!(
        restarted.next_due_epoch_for_capabilities([account.clone()], 2_000),
        Some(5_000)
    );
    let forced_early = restarted.request_refresh(&account, 2, true, 2_000).unwrap();
    assert_eq!(forced_early.generation, 2);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);
    assert!(
        restarted
            .poll_due_for_capabilities([account.clone()], 2_000)
            .is_empty()
    );
    restart_clock.advance(Duration::from_secs(2_999));
    assert_eq!(
        restarted.next_due_epoch_for_capabilities([account.clone()], 4_999),
        Some(5_000)
    );
    let before_retry_after = restarted.request_refresh(&account, 2, true, 4_999).unwrap();
    assert_eq!(before_retry_after.generation, 2);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);
    assert!(
        restarted
            .poll_due_for_capabilities([account.clone()], 4_999)
            .is_empty()
    );
    restart_clock.advance(Duration::from_secs(1));
    let due = restarted.poll_due_for_capabilities([account.clone()], 5_000);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].generation, 3);
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
    let clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        2_000,
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
    )]
    let clock_port: Arc<dyn MonotonicClock> = clock.clone();
    let coordinator = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig::default(),
        None,
        None,
        clock_port,
    );

    let recovered = coordinator
        .request_refresh(&account, 0, true, 2_000)
        .unwrap();
    assert_eq!(recovered.generation, 4);
    assert_eq!(recovered.phase, UsageRefreshPhase::Failed);
    assert_eq!(recovered.retry_at_epoch, Some(2_300));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    clock.advance(Duration::from_secs(299));
    let too_early = coordinator
        .request_refresh(&account, 0, true, 2_299)
        .unwrap();
    assert_eq!(too_early.generation, 4);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    clock.advance(Duration::from_secs(1));
    let recovered = coordinator
        .request_refresh(&account, 0, true, 2_300)
        .unwrap();
    executor.wait_started(1);
    let joiner = coordinator
        .request_refresh(&account, 0, true, 2_300)
        .unwrap();
    assert_eq!(recovered.generation, 5);
    assert_eq!(joiner.generation, 5);
    assert!(joiner.phase.is_active());

    executor.release(1);
    let terminal = join_ok(&coordinator, &account, 5, 2_301);
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn coordinator_recovery_does_not_treat_queued_work_as_a_provider_attempt() {
    let account = capability("queued-owner-loss");
    let store = Arc::new(MemoryStore::default());
    let mut queued = AccountStateEnvelope::idle(account.clone());
    queued.generation = 4;
    queued.phase = UsageRefreshPhase::Queued;
    queued.started_at_epoch = Some(1_000);
    store.states.lock().unwrap().insert(account.clone(), queued);
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(2_000, 80),
    )));
    let clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        2_000,
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
    )]
    let clock_port: Arc<dyn MonotonicClock> = clock.clone();
    let coordinator = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig::default(),
        None,
        None,
        clock_port,
    );

    let recovered = coordinator.current(&account, 2_000).unwrap();
    assert_eq!(recovered.phase, UsageRefreshPhase::Failed);
    assert_eq!(
        recovered.error.as_ref().unwrap().kind,
        UsageCoordinationErrorKind::OwnerLost
    );
    assert!(
        recovered
            .retry_at_epoch
            .is_some_and(|deadline| { deadline > 2_000 && deadline < 2_300 })
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn revoked_updating_attempt_cooldown_survives_remove_readd_and_restart() {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(FileAccountStateStore::at(temp.path().join("accounts")));
    let account = capability("revoked-in-flight");
    let first_clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::new(
        1_000,
        900_000_000,
    )));
    let first_executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let first_provider: Arc<dyn UsageProviderExecutor> = first_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let first_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
    )]
    let first_clock_port: Arc<dyn MonotonicClock> = first_clock.clone();
    let first = UsageCoordinator::start_with_clock(
        first_provider,
        first_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-a".into())])),
        None,
        first_clock_port,
    );
    first.request_refresh(&account, 0, true, 1_000).unwrap();
    first_executor.wait_started(1);

    first.reconcile_catalog([], 1_000).unwrap();
    let tombstone = store.load(&account, 1_001).unwrap().unwrap();
    assert_eq!(tombstone.phase, UsageRefreshPhase::Updating);
    assert!(tombstone.terminal_result.is_none());
    assert!(tombstone.last_good.is_none());
    assert_eq!(tombstone.retry_deadline_epoch, Some(1_301));
    first_executor.release(1);
    first_executor.wait_idle();
    let tombstone = wait_for_idle_cooldown_tombstone(&*store, &account, 1_301, 1_001);
    assert!(tombstone.terminal_result.is_none());
    assert!(tombstone.last_good.is_none());
    drop(first);

    let restarted_clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::new(
        1_000,
        900_000_000,
    )));
    let restarted_executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_301, 79),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let restarted_provider: Arc<dyn UsageProviderExecutor> = restarted_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let restarted_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
    )]
    let restarted_clock_port: Arc<dyn MonotonicClock> = restarted_clock.clone();
    let restarted = UsageCoordinator::start_with_clock(
        restarted_provider,
        restarted_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-b".into())])),
        None,
        restarted_clock_port,
    );
    let current = restarted.current(&account, 1_001).unwrap();
    assert_eq!(current.phase, UsageRefreshPhase::Idle);
    let too_early = restarted
        .request_refresh(&account, current.generation, true, 1_300)
        .unwrap();
    assert_eq!(too_early.generation, current.generation);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);

    restarted_clock.advance(Duration::from_millis(299_900));
    let too_early = restarted
        .request_refresh(&account, current.generation, true, 1_300)
        .unwrap();
    assert_eq!(too_early.generation, current.generation);
    restarted_clock.advance(Duration::from_millis(200));
    let allowed = restarted
        .request_refresh(&account, current.generation, true, 1_301)
        .unwrap();
    assert_eq!(allowed.generation, current.generation + 1);
    assert_eq!(
        join_ok(&restarted, &account, allowed.generation, 1_302).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn revoked_attempt_waits_for_delayed_probe_and_completion_cooldown_before_readd() {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(FileAccountStateStore::at(temp.path().join("accounts")));
    let account = capability("revoked-before-probe");
    let clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::new(
        1_000,
        900_000_000,
    )));
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
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

    let (before_probe, probe_waiting) = mpsc::channel();
    let (resume_probe, probe_resume) = mpsc::channel();
    let probe_resume = Arc::new(Mutex::new(probe_resume));
    let hook_waiting = before_probe.clone();
    let hook_resume = Arc::clone(&probe_resume);
    let hook_calls = Arc::new(AtomicUsize::new(0));
    let hook_run_count = Arc::clone(&hook_calls);
    *coordinator.shared.before_provider_call_hook.lock().unwrap() = Some(Arc::new(move || {
        if hook_run_count.fetch_add(1, Ordering::SeqCst) == 0 {
            hook_waiting.send(()).unwrap();
            hook_resume.lock().unwrap().recv().unwrap();
        }
    }));

    coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    probe_waiting
        .recv_timeout(Duration::from_secs(2))
        .expect("worker did not reach the pre-probe boundary");
    coordinator.reconcile_catalog([], 1_000).unwrap();
    let pending_tombstone = store.load(&account, 1_001).unwrap().unwrap();
    assert_eq!(pending_tombstone.phase, UsageRefreshPhase::Updating);
    assert!(pending_tombstone.terminal_result.is_none());
    assert!(pending_tombstone.last_good.is_none());

    // This second removal happens after the original retry floor expires but
    // before the delayed worker reaches the provider. The pending marker must
    // survive even though the account is already revoked.
    clock.advance(Duration::from_secs(301));
    coordinator.reconcile_catalog([], 1_301).unwrap();
    let repeated_tombstone = store.load(&account, 1_301).unwrap().unwrap();
    assert_eq!(repeated_tombstone.phase, UsageRefreshPhase::Updating);
    assert!(repeated_tombstone.terminal_result.is_none());
    assert!(repeated_tombstone.last_good.is_none());
    assert!(repeated_tombstone.terminal_error.is_none());
    assert!(
        account_cooldown_deadline(&repeated_tombstone)
            .is_some_and(|deadline| { deadline <= clock.sample(1_301).ceil_epoch() })
    );

    coordinator
        .reconcile_catalog([catalog_entry(&account, "revision-b")], 1_301)
        .unwrap();
    let readded_pending = store.load(&account, 1_301).unwrap().unwrap();
    assert_eq!(readded_pending.phase, UsageRefreshPhase::Updating);
    assert!(readded_pending.terminal_result.is_none());
    assert!(readded_pending.last_good.is_none());
    assert!(readded_pending.terminal_error.is_none());
    assert_restarted_pending_fence(&account, &readded_pending, &clock);

    let current = coordinator.current(&account, 1_301).unwrap();
    let blocked = coordinator
        .request_refresh(&account, current.generation, true, 1_301)
        .unwrap();
    assert_eq!(blocked.generation, current.generation);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    // The executor begins 400 seconds after the Updating reservation. A
    // tombstone based only on removal time would already have expired.
    clock.advance(Duration::from_secs(99));
    resume_probe.send(()).unwrap();
    executor.wait_started(1);
    let first_start = clock.sample(1_400);
    assert_eq!(first_start.wall_epoch, Duration::new(1_400, 900_000_000));
    let still_blocked = coordinator
        .request_refresh(&account, current.generation, true, 1_400)
        .unwrap();
    assert_eq!(still_blocked.generation, current.generation);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    executor.release(1);
    executor.wait_idle();
    let finished = wait_for_idle_cooldown_tombstone(&*store, &account, 1_701, 1_401);
    assert!(finished.terminal_result.is_none());
    assert!(finished.last_good.is_none());

    clock.advance(Duration::from_mins(5));
    let early = coordinator
        .request_refresh(&account, current.generation, true, 1_700)
        .unwrap();
    assert_eq!(early.generation, current.generation);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    executor.set_outcome(ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::RateLimited,
        message: "provider requested a later retry".into(),
        retry_at_epoch: Some(2_400),
    });
    clock.advance(Duration::from_millis(100));
    let allowed = coordinator
        .request_refresh(&account, current.generation, true, 1_701)
        .unwrap();
    assert_eq!(allowed.generation, current.generation + 1);
    executor.wait_started(2);
    let second_start = clock.sample(1_701);
    assert!(second_start.monotonic.saturating_sub(first_start.monotonic) >= Duration::from_mins(5));
    coordinator.reconcile_catalog([], 1_701).unwrap();
    executor.release(1);
    executor.wait_idle();
    let finished = wait_for_idle_cooldown_tombstone(&*store, &account, 2_400, 2_400);
    assert_eq!(finished.rate_limit_deadline_epoch, Some(2_400));
    assert!(finished.terminal_error.is_none());
}

fn assert_restarted_pending_fence(
    account: &UsageAccountCapability,
    readded_pending: &AccountStateEnvelope,
    clock: &Arc<FakeMonotonicClock>,
) {
    let restart_store = Arc::new(MemoryStore::default());
    restart_store.store(readded_pending, 1_301).unwrap();
    let restart_executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_301, 80),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce restart fixture ports to coordinator trait objects"
    )]
    let restart_provider: Arc<dyn UsageProviderExecutor> = restart_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce restart fixture ports to coordinator trait objects"
    )]
    let restart_state_store: Arc<dyn AccountStateStore> = restart_store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "share fake clock with the restart fixture"
    )]
    let restart_clock: Arc<dyn MonotonicClock> = clock.clone();
    let restarted = UsageCoordinator::start_with_clock(
        restart_provider,
        restart_state_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-b".into())])),
        None,
        restart_clock,
    );
    let recovered = restarted.current(account, 1_301).unwrap();
    assert_eq!(recovered.phase, UsageRefreshPhase::Failed);
    assert_eq!(recovered.retry_at_epoch, Some(1_602));
    let restarted_early = restarted
        .request_refresh(account, recovered.generation, true, 1_301)
        .unwrap();
    assert_eq!(restarted_early.generation, recovered.generation);
    assert_eq!(restart_executor.calls.load(Ordering::SeqCst), 0);
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
fn catalog_revocation_clears_materialized_results_but_preserves_cooldown_and_fences_late_result() {
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
    assert!(revoked.snapshot.is_none());
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
        "pending attempt tombstone replaces the Updating record before purge"
    );
    let tombstone = store.states.lock().unwrap().get(&account).cloned().unwrap();
    assert_eq!(tombstone.phase, UsageRefreshPhase::Updating);
    assert!(tombstone.terminal_result.is_none());
    assert!(tombstone.last_good.is_none());
    assert!(tombstone.terminal_error.is_none());
    assert_eq!(tombstone.provider_invoked_at_epoch, Some(1_400));
    assert_eq!(tombstone.success_deadline_epoch, Some(1_300));
    assert_eq!(tombstone.retry_deadline_epoch, Some(1_701));

    executor.release(1);
    executor.wait_idle();
    let after_late_result = coordinator.current(&account, 1_402).unwrap();
    assert_eq!(
        after_late_result.error.as_ref().unwrap().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
    assert_eq!(after_late_result.generation, revoked.generation);
    assert!(after_late_result.snapshot.is_none());
    let finished_tombstone = wait_for_idle_cooldown_tombstone(&*store, &account, 1_701, 1_402);
    assert!(finished_tombstone.terminal_error.is_none());
    assert!(finished_tombstone.terminal_result.is_none());
    assert_eq!(
        coordinator
            .request_refresh(&account, revoked.generation, true, 1_402)
            .unwrap_err()
            .kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
}

#[test]
fn catalog_revision_change_replaces_old_state_with_cooldown_tombstone() {
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
    assert!(store.purges.lock().unwrap().is_empty());
    let tombstone = store.load(&account, 1_002).unwrap().unwrap();
    assert_eq!(tombstone.phase, UsageRefreshPhase::Idle);
    assert!(tombstone.terminal_result.is_none());
    assert!(tombstone.last_good.is_none());
    assert!(account_cooldown_deadline(&tombstone).is_some_and(|deadline| deadline > 1_002));

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
    let account = UsageAccountCapability {
        account_id: "account-a".into(),
        surface_id: "codex".into(),
    };
    let coordinator = UsageCoordinator::with_catalog(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<MemoryStore>::clone(&store),
        UsageCoordinatorConfig {
            success_cooldown: Duration::ZERO,
            ..UsageCoordinatorConfig::default()
        },
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
fn catalog_cooldown_tombstone_prevents_restart_resurrection() {
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
    let tombstone = store.load(&account, 1_002).unwrap().unwrap();
    assert_eq!(tombstone.phase, UsageRefreshPhase::Idle);
    assert!(tombstone.terminal_result.is_none());
    assert!(tombstone.last_good.is_none());
    assert!(tombstone.terminal_error.is_none());
    assert!(account_cooldown_deadline(&tombstone).is_some_and(|deadline| deadline > 1_002));
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

#[test]
fn corrupt_revoked_account_is_quarantined_without_poisoning_rotation() {
    let temp = tempfile::tempdir().unwrap();
    let accounts = temp.path().join("accounts");
    std::fs::create_dir_all(&accounts).unwrap();
    let account = capability("account-a");
    let active = accounts.join("claude-account-a.json");
    std::fs::write(&active, b"corrupt revoked state").unwrap();
    std::fs::set_permissions(&active, std::fs::Permissions::from_mode(0o600)).unwrap();

    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(FileAccountStateStore::at(accounts.clone()));
    let coordinator = UsageCoordinator::with_catalog(
        executor,
        store,
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "revision-a")],
    );

    coordinator.reconcile_catalog([], 1_001).unwrap();
    assert!(!active.exists());
    let quarantined = std::fs::read_dir(accounts)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".corrupt."))
        .collect::<Vec<_>>();
    assert_eq!(quarantined.len(), 1);
    assert_eq!(
        coordinator.current(&account, 1_001).unwrap_err().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
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

#[test]
fn cadence_tiers_select_spec_intervals_with_bounded_jitter() {
    let account = capability("account-a");
    let cases = [
        (UsageActivity::DirectInteraction, false, 120..=150),
        (UsageActivity::Recent, false, 300..=375),
        (UsageActivity::Idle, false, 900..=1_125),
        (UsageActivity::LongIdle, false, 1_800..=2_250),
        (UsageActivity::DirectInteraction, true, 1_800..=2_250),
    ];
    for (activity, low_power, range) in cases {
        let first = cadence_deadline(activity, low_power, &account, 3, 10_000);
        assert!(
            range.contains(&(first - 10_000)),
            "{activity:?} low_power={low_power} out of range: {first}"
        );
        assert_eq!(
            first,
            cadence_deadline(activity, low_power, &account, 3, 10_000),
            "cadence deadline must be deterministic for joined callers"
        );
    }
}

#[test]
fn cadence_poll_due_fires_once_per_interval_and_honors_success_cooldown() {
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        cadence_quota_view(1_000),
    )));
    let coordinator = cadence_coordinator(Arc::clone(&executor), UsageCoordinatorConfig::default());
    let account = capability("account-a");
    coordinator
        .set_activity(&account, UsageActivity::DirectInteraction, false, 1_000)
        .unwrap();

    let started = coordinator.poll_due(1_000);
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].generation, 1);
    assert_eq!(
        join_ok(&coordinator, &account, 1, 1_001).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    let due = coordinator.next_due_epoch().unwrap();
    assert!((1_300..=1_310).contains(&due), "hard minimum due={due}");
    assert!(coordinator.poll_due(due - 1).is_empty());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    let second = coordinator.poll_due(due);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].generation, 2);
    assert_eq!(
        join_ok(&coordinator, &account, 2, due + 1).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn cadence_wake_recalculates_without_missed_poll_burst() {
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        cadence_quota_view(1_000),
    )));
    let coordinator = cadence_coordinator(Arc::clone(&executor), UsageCoordinatorConfig::default());
    let account = capability("account-a");
    coordinator
        .set_activity(&account, UsageActivity::DirectInteraction, false, 1_000)
        .unwrap();
    assert_eq!(coordinator.poll_due(1_000).len(), 1);
    drop(join_ok(&coordinator, &account, 1, 1_001));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    let wake = 1_000 + 36_000;
    assert_eq!(coordinator.note_wake(wake), 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let due = coordinator.next_due_epoch().unwrap();
    assert!((wake + 120..=wake + 150).contains(&due), "due={due}");
    assert!(coordinator.poll_due(wake).is_empty());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    let second = coordinator.poll_due(due);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].generation, 2);
    drop(join_ok(&coordinator, &account, 2, due + 1));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn cadence_shared_retry_after_wins_over_periodic_due() {
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::RateLimited,
        message: "provider rate limited".into(),
        retry_at_epoch: Some(5_000),
    }));
    let coordinator = cadence_coordinator(Arc::clone(&executor), UsageCoordinatorConfig::default());
    let account = capability("account-a");
    coordinator
        .set_activity(&account, UsageActivity::DirectInteraction, false, 1_000)
        .unwrap();
    assert_eq!(coordinator.poll_due(1_000).len(), 1);
    let failed = join_ok(&coordinator, &account, 1, 1_001);
    assert_eq!(failed.phase, UsageRefreshPhase::Failed);
    assert_eq!(failed.retry_at_epoch, Some(5_000));

    let due = coordinator.next_due_epoch().unwrap();
    assert_eq!(due, 5_000, "provider deadline must win over cadence");
    assert!(coordinator.poll_due(4_999).is_empty());

    executor.set_outcome(ProviderProbeOutcome::success(cadence_quota_view(5_000)));
    let second = coordinator.poll_due(5_000);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].generation, 2);
    assert_eq!(
        join_ok(&coordinator, &account, 2, 5_001).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn cadence_two_clients_single_flight_exactly_one_provider_call() {
    let executor = Arc::new(CadenceGateExecutor::new(ProviderProbeOutcome::success(
        cadence_quota_view(1_000),
    )));
    let coordinator = cadence_coordinator(Arc::clone(&executor), UsageCoordinatorConfig::default());
    let account = capability("account-a");
    coordinator
        .set_activity(&account, UsageActivity::DirectInteraction, false, 1_000)
        .unwrap();

    let winner = coordinator.poll_due(1_000);
    assert_eq!(winner.len(), 1);
    assert_eq!(winner[0].generation, 1);
    executor.wait_started();

    let manual_joiner = coordinator
        .request_refresh(&account, 1, true, 1_000)
        .unwrap();
    assert_eq!(manual_joiner.generation, 1);
    assert!(manual_joiner.phase.is_active());
    let repeated_manual = coordinator
        .request_refresh(&account, 1, true, 1_000)
        .unwrap();
    assert_eq!(repeated_manual.generation, 1);
    let due = coordinator.next_due_epoch().unwrap();
    let ambient_joiner = coordinator.poll_due(due);
    assert_eq!(ambient_joiner.len(), 1);
    assert_eq!(ambient_joiner[0].generation, 1);

    executor.release();
    let terminal = join_ok(&coordinator, &account, 1, 1_001);
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TombstoneStoreEvent {
    Store {
        capability: UsageAccountCapability,
        phase: UsageRefreshPhase,
        generation: u64,
        result_free: bool,
        cooldown_deadline_epoch: Option<i64>,
    },
    Purge(UsageAccountCapability),
}

struct TombstoneWriteFailureStore {
    inner: MemoryStore,
    fail_tombstone: AtomicUsize,
    fail_purge_after_removal: AtomicUsize,
    events: Mutex<Vec<TombstoneStoreEvent>>,
}

impl AccountStateStore for TombstoneWriteFailureStore {
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
        self.events
            .lock()
            .unwrap()
            .push(TombstoneStoreEvent::Store {
                capability: envelope.capability.clone(),
                phase: envelope.phase,
                generation: envelope.generation,
                result_free: envelope.terminal_result.is_none()
                    && envelope.last_good.is_none()
                    && envelope.terminal_error.is_none(),
                cooldown_deadline_epoch: account_cooldown_deadline(envelope),
            });
        let is_tombstone = envelope.phase == UsageRefreshPhase::Idle
            && envelope.terminal_result.is_none()
            && envelope.last_good.is_none()
            && envelope.terminal_error.is_none()
            && account_cooldown_deadline(envelope).is_some_and(|deadline| deadline > now_epoch);
        if is_tombstone && self.fail_tombstone.swap(0, Ordering::SeqCst) > 0 {
            return Err(StateStoreError::Unavailable);
        }
        self.inner.store(envelope, now_epoch)
    }

    fn purge(&self, capability: &UsageAccountCapability) -> Result<(), StateStoreError> {
        self.events
            .lock()
            .unwrap()
            .push(TombstoneStoreEvent::Purge(capability.clone()));
        self.inner.purge(capability)?;
        if self.fail_purge_after_removal.swap(0, Ordering::SeqCst) > 0 {
            return Err(StateStoreError::Unavailable);
        }
        Ok(())
    }
}

fn removed_account_cooldown_survives_restart(
    account_id: &str,
    outcome: ProviderProbeOutcome,
    config: UsageCoordinatorConfig,
) {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(FileAccountStateStore::at(temp.path().join("accounts")));
    let account = capability(account_id);
    let first_executor = Arc::new(ImmediateExecutor::new(outcome.clone()));
    let first = UsageCoordinator::with_catalog(
        Arc::<ImmediateExecutor>::clone(&first_executor),
        Arc::<FileAccountStateStore>::clone(&store),
        config,
        [catalog_entry(&account, "revision-a")],
    );

    let queued = first.request_refresh(&account, 0, true, 1_000).unwrap();
    let terminal = join_ok(&first, &account, queued.generation, 1_001);
    assert!(terminal.phase.is_terminal());
    assert_eq!(first_executor.calls.load(Ordering::SeqCst), 1);
    let before_removal = store.load(&account, 1_001).unwrap().unwrap();
    let deadline = account_cooldown_deadline(&before_removal).expect("account cooldown");
    assert!(deadline > 1_100);
    match &outcome {
        ProviderProbeOutcome::Success(_) => {
            assert!(before_removal.provider_invoked_at_epoch.is_some());
            assert_eq!(before_removal.success_deadline_epoch, Some(deadline));
        }
        ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::RateLimited,
            retry_at_epoch: Some(provider_deadline),
            ..
        } => {
            assert_eq!(
                before_removal.rate_limit_deadline_epoch,
                Some(*provider_deadline)
            );
            assert_eq!(
                before_removal.retry_deadline_epoch,
                Some(*provider_deadline)
            );
        }
        ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::ProviderUnavailable,
            retry_at_epoch: Some(provider_deadline),
            ..
        } => {
            assert_eq!(before_removal.rate_limit_deadline_epoch, None);
            assert_eq!(
                before_removal.retry_deadline_epoch,
                Some(*provider_deadline)
            );
        }
        ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::ProviderUnavailable,
            retry_at_epoch: None,
            ..
        } => {
            assert!(
                before_removal
                    .retry_deadline_epoch
                    .is_some_and(|retry| retry > 1_300)
            );
            assert!(before_removal.rate_limit_deadline_epoch.is_none());
        }
        ProviderProbeOutcome::Failure { .. } => panic!("unexpected test outcome"),
    }

    first.reconcile_catalog([], 1_100).unwrap();
    let tombstone = store.load(&account, 1_100).unwrap().unwrap();
    assert_eq!(tombstone.generation, queued.generation.saturating_add(1));
    assert_eq!(tombstone.phase, UsageRefreshPhase::Idle);
    assert!(tombstone.terminal_result.is_none());
    assert!(tombstone.last_good.is_none());
    assert!(tombstone.terminal_error.is_none());
    assert_eq!(account_cooldown_deadline(&tombstone), Some(deadline));
    assert_eq!(
        tombstone.provider_invoked_at_epoch,
        before_removal.provider_invoked_at_epoch
    );
    assert_eq!(
        tombstone.rate_limit_deadline_epoch,
        before_removal.rate_limit_deadline_epoch
    );
    let expected_retry_deadline = before_removal
        .retry_deadline_epoch
        .map_or(1_300, |deadline| deadline.max(1_300));
    assert_eq!(
        tombstone.retry_deadline_epoch,
        Some(expected_retry_deadline)
    );
    assert_eq!(
        tombstone.success_deadline_epoch,
        before_removal.success_deadline_epoch
    );
    assert_eq!(
        tombstone.consecutive_failures,
        before_removal.consecutive_failures
    );
    drop(first);

    let second_executor = Arc::new(ImmediateExecutor::new(outcome));
    let recovery_epoch = 1_101;
    let restart_clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        u64::try_from(recovery_epoch).unwrap(),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce restart fake clock to the coordinator clock port"
    )]
    let restart_clock_port: Arc<dyn MonotonicClock> = restart_clock.clone();
    let second = UsageCoordinator::start_with_clock(
        Arc::<ImmediateExecutor>::clone(&second_executor),
        Arc::<FileAccountStateStore>::clone(&store),
        config,
        Some(BTreeMap::from([(account.clone(), "revision-b".into())])),
        None,
        restart_clock_port,
    );
    let readded = second.current(&account, recovery_epoch).unwrap();
    assert_eq!(readded.phase, UsageRefreshPhase::Idle);
    assert!(readded.snapshot.is_none());
    assert!(readded.error.is_none());
    assert_eq!(readded.generation, tombstone.generation);
    let recovery_floor_epoch = recovery_epoch.saturating_add(300);
    let effective_deadline = deadline.max(recovery_floor_epoch);
    assert_eq!(
        second.next_due_epoch_for_capabilities([account.clone()], recovery_epoch),
        Some(effective_deadline)
    );

    let suppressed_at_recovery = second
        .request_refresh(&account, readded.generation, true, recovery_epoch)
        .unwrap();
    assert_eq!(suppressed_at_recovery.generation, readded.generation);
    assert_eq!(second_executor.calls.load(Ordering::SeqCst), 0);

    restart_clock.advance(Duration::from_secs(
        u64::try_from(effective_deadline - recovery_epoch - 1).unwrap(),
    ));
    let suppressed_before_deadline = second
        .request_refresh(&account, readded.generation, true, effective_deadline - 1)
        .unwrap();
    assert_eq!(suppressed_before_deadline.generation, readded.generation);
    assert_eq!(second_executor.calls.load(Ordering::SeqCst), 0);

    restart_clock.advance(Duration::from_secs(1));
    let at_deadline = second
        .request_refresh(&account, readded.generation, true, effective_deadline)
        .unwrap();
    assert_eq!(at_deadline.generation, readded.generation.saturating_add(1));
    let after_deadline = join_ok(
        &second,
        &account,
        at_deadline.generation,
        effective_deadline + 1,
    );
    assert!(after_deadline.phase.is_terminal());
    assert_eq!(second_executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn claude_active_recovery_floor_survives_remove_after_forward_wall_jump() {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(FileAccountStateStore::at(temp.path().join("accounts")));
    let account = capability("recovery-floor-removed-after-wall-jump");
    store
        .store(&completed_claude_attempt(&account, 1_000, 1_300), 1_001)
        .unwrap();

    let first_clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        5_000,
    )));
    let first_executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(5_300, 80),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to shared trait object"
    )]
    let first_provider: Arc<dyn UsageProviderExecutor> = first_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce file store to shared trait object"
    )]
    let first_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
    )]
    let first_clock_port: Arc<dyn MonotonicClock> = first_clock.clone();
    let first = UsageCoordinator::start_with_clock(
        first_provider,
        first_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-a".into())])),
        None,
        first_clock_port,
    );

    assert_eq!(first.current(&account, 5_000).unwrap().generation, 1);
    first_clock.jump_wall_forward(Duration::from_hours(1));
    first.reconcile_catalog([], 8_600).unwrap();
    assert_eq!(first_executor.calls.load(Ordering::SeqCst), 0);
    let tombstone = store.load(&account, 8_600).unwrap().unwrap();
    assert_eq!(tombstone.phase, UsageRefreshPhase::Idle);
    assert!(tombstone.terminal_result.is_none());
    assert!(tombstone.last_good.is_none());
    assert!(tombstone.terminal_error.is_none());
    assert_eq!(tombstone.provider_invoked_at_epoch, Some(1_000));
    assert_eq!(tombstone.retry_deadline_epoch, Some(8_900));
    assert_eq!(account_cooldown_deadline(&tombstone), Some(8_900));
    drop(first);

    let restart_clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        8_600,
    )));
    let restarted_executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(8_900, 79),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to shared trait object"
    )]
    let restarted_provider: Arc<dyn UsageProviderExecutor> = restarted_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce file store to shared trait object"
    )]
    let restarted_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
    )]
    let restart_clock_port: Arc<dyn MonotonicClock> = restart_clock.clone();
    let restarted = UsageCoordinator::start_with_clock(
        restarted_provider,
        restarted_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-b".into())])),
        None,
        restart_clock_port,
    );
    let readded = restarted.current(&account, 8_600).unwrap();
    assert_eq!(readded.phase, UsageRefreshPhase::Idle);
    assert_eq!(readded.generation, tombstone.generation);
    assert!(readded.snapshot.is_none());
    assert!(readded.error.is_none());
    assert_eq!(
        restarted.next_due_epoch_for_capabilities([account.clone()], 8_600),
        Some(8_900)
    );
    assert!(
        restarted
            .poll_due_for_capabilities([account.clone()], 8_600)
            .is_empty()
    );
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);

    restart_clock.advance(Duration::from_secs(299));
    let early = restarted
        .request_refresh(&account, readded.generation, true, 8_899)
        .unwrap();
    assert_eq!(early.generation, readded.generation);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        restarted.next_due_epoch_for_capabilities([account.clone()], 8_899),
        Some(8_900)
    );

    restart_clock.advance(Duration::from_secs(1));
    let due = restarted.poll_due_for_capabilities([account.clone()], 8_900);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].generation, readded.generation.saturating_add(1));
    assert_eq!(
        join_ok(&restarted, &account, due[0].generation, 8_901).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn claude_lazy_account_removal_preserves_expired_invocation_after_wall_jump() {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(FileAccountStateStore::at(temp.path().join("accounts")));
    let account = capability("lazy-recovery-floor-removed-after-wall-jump");
    store
        .store(&completed_claude_attempt(&account, 1_000, 1_300), 1_001)
        .unwrap();

    let first_clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        5_000,
    )));
    let first_executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(5_300, 80),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to shared trait object"
    )]
    let first_provider: Arc<dyn UsageProviderExecutor> = first_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce file store to shared trait object"
    )]
    let first_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
    )]
    let first_clock_port: Arc<dyn MonotonicClock> = first_clock.clone();
    let first = UsageCoordinator::start_with_clock(
        first_provider,
        first_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-a".into())])),
        None,
        first_clock_port,
    );

    // Leave the account unloaded: removal must retain prior invocation
    // evidence from the durable preimage without relying on `current()`.
    first_clock.jump_wall_forward(Duration::from_hours(1));
    first.reconcile_catalog([], 8_600).unwrap();
    assert_eq!(first_executor.calls.load(Ordering::SeqCst), 0);
    let tombstone = store.load(&account, 8_600).unwrap().unwrap();
    assert_eq!(tombstone.phase, UsageRefreshPhase::Idle);
    assert!(tombstone.terminal_result.is_none());
    assert!(tombstone.last_good.is_none());
    assert!(tombstone.terminal_error.is_none());
    assert_eq!(tombstone.provider_invoked_at_epoch, Some(1_000));
    assert_eq!(tombstone.retry_deadline_epoch, Some(8_900));
    drop(first);

    let restart_clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        8_600,
    )));
    let restarted_executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(8_900, 79),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to shared trait object"
    )]
    let restarted_provider: Arc<dyn UsageProviderExecutor> = restarted_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce file store to shared trait object"
    )]
    let restarted_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
    )]
    let restart_clock_port: Arc<dyn MonotonicClock> = restart_clock.clone();
    let restarted = UsageCoordinator::start_with_clock(
        restarted_provider,
        restarted_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(account.clone(), "revision-b".into())])),
        None,
        restart_clock_port,
    );
    let readded = restarted.current(&account, 8_600).unwrap();
    assert_eq!(readded.phase, UsageRefreshPhase::Idle);
    assert_eq!(readded.generation, tombstone.generation);
    assert_eq!(
        restarted.next_due_epoch_for_capabilities([account.clone()], 8_600),
        Some(8_900)
    );
    assert!(
        restarted
            .poll_due_for_capabilities([account.clone()], 8_600)
            .is_empty()
    );
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);

    restart_clock.advance(Duration::from_secs(299));
    let forced_early = restarted
        .request_refresh(&account, readded.generation, true, 8_899)
        .unwrap();
    assert_eq!(forced_early.generation, readded.generation);
    assert!(
        restarted
            .poll_due_for_capabilities([account.clone()], 8_899)
            .is_empty()
    );
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);

    restart_clock.advance(Duration::from_secs(1));
    let due = restarted.poll_due_for_capabilities([account.clone()], 8_900);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].generation, readded.generation.saturating_add(1));
    assert_eq!(
        join_ok(&restarted, &account, due[0].generation, 8_901).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn claude_invocation_floor_survives_remove_readd_and_restart() {
    removed_account_cooldown_survives_restart(
        "floor-account",
        ProviderProbeOutcome::success(quota_view(1_000, 80)),
        UsageCoordinatorConfig {
            success_cooldown: Duration::from_secs(1),
            ..UsageCoordinatorConfig::default()
        },
    );
}

#[test]
fn rate_limit_retry_after_survives_remove_readd_and_restart() {
    removed_account_cooldown_survives_restart(
        "rate-limit-account",
        ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::RateLimited,
            message: "provider rate limited".into(),
            retry_at_epoch: Some(5_000),
        },
        UsageCoordinatorConfig::default(),
    );
}

#[test]
fn general_provider_retry_after_survives_remove_readd_and_restart() {
    removed_account_cooldown_survives_restart(
        "provider-retry-account",
        ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::ProviderUnavailable,
            message: "provider asked for a later retry".into(),
            retry_at_epoch: Some(4_000),
        },
        UsageCoordinatorConfig::default(),
    );
}

#[test]
fn local_exponential_backoff_survives_remove_readd_and_restart() {
    let config = UsageCoordinatorConfig {
        retry_policy: UsagePolicy {
            retry_base: Duration::from_mins(10),
            retry_cap: Duration::from_mins(10),
            ..UsagePolicy::default()
        },
        ..UsageCoordinatorConfig::default()
    };
    removed_account_cooldown_survives_restart(
        "backoff-account",
        ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::ProviderUnavailable,
            message: "provider unavailable".into(),
            retry_at_epoch: None,
        },
        config,
    );
}

#[test]
fn failed_tombstone_write_restores_catalog_and_preimage() {
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(TombstoneWriteFailureStore {
        inner: MemoryStore::default(),
        fail_tombstone: AtomicUsize::new(0),
        fail_purge_after_removal: AtomicUsize::new(0),
        events: Mutex::new(Vec::new()),
    });
    let account = capability("rollback-account");
    let coordinator = UsageCoordinator::with_catalog(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<TombstoneWriteFailureStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "revision-a")],
    );
    let queued = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    assert_eq!(
        join_ok(&coordinator, &account, queued.generation, 1_001).phase,
        UsageRefreshPhase::Completed
    );
    let preimage = store.inner.load(&account, 1_001).unwrap().unwrap();
    store.fail_tombstone.store(1, Ordering::SeqCst);

    let error = coordinator.reconcile_catalog([], 1_002).unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(store.inner.load(&account, 1_002).unwrap(), Some(preimage));
    let restored = coordinator.current(&account, 1_002).unwrap();
    assert_eq!(restored.phase, UsageRefreshPhase::Completed);
    assert_eq!(
        restored.snapshot.unwrap().buckets[0].remaining_percent,
        Some(80)
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_purge_after_removal_restores_catalog_and_preimage() {
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(TombstoneWriteFailureStore {
        inner: MemoryStore::default(),
        fail_tombstone: AtomicUsize::new(0),
        fail_purge_after_removal: AtomicUsize::new(0),
        events: Mutex::new(Vec::new()),
    });
    let account = UsageAccountCapability {
        account_id: "purge-rollback-account".into(),
        surface_id: "codex".into(),
    };
    let coordinator = UsageCoordinator::with_catalog(
        Arc::<ImmediateExecutor>::clone(&executor),
        Arc::<TombstoneWriteFailureStore>::clone(&store),
        UsageCoordinatorConfig {
            success_cooldown: Duration::ZERO,
            ..UsageCoordinatorConfig::default()
        },
        [catalog_entry(&account, "revision-a")],
    );
    let queued = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    assert_eq!(
        join_ok(&coordinator, &account, queued.generation, 1_001).phase,
        UsageRefreshPhase::Completed
    );
    let preimage = store.inner.load(&account, 1_001).unwrap().unwrap();
    store.fail_purge_after_removal.store(1, Ordering::SeqCst);

    let error = coordinator.reconcile_catalog([], 1_002).unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(store.inner.load(&account, 1_002).unwrap(), Some(preimage));
    let restored = coordinator.current(&account, 1_002).unwrap();
    assert_eq!(restored.phase, UsageRefreshPhase::Completed);
    assert_eq!(
        restored.snapshot.unwrap().buckets[0].remaining_percent,
        Some(80)
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

fn assert_later_purge_store_order(
    store: &TombstoneWriteFailureStore,
    pending_preimage: &AccountStateEnvelope,
    restored_pending: &AccountStateEnvelope,
    pending_preimage_missing: bool,
    terminal_preimage: &AccountStateEnvelope,
    later_purge_preimage: &AccountStateEnvelope,
) {
    let events = store.events.lock().unwrap().clone();
    assert_eq!(events.len(), 6);
    assert!(matches!(
        &events[0],
        TombstoneStoreEvent::Store {
            capability,
            phase: UsageRefreshPhase::Updating,
            generation,
            result_free: true,
            ..
        } if capability == &pending_preimage.capability
            && *generation > pending_preimage.generation
    ));
    let marker_generation = match &events[0] {
        TombstoneStoreEvent::Store { generation, .. } => *generation,
        event @ TombstoneStoreEvent::Purge(_) => {
            panic!("expected pending marker write first, got {event:?}");
        }
    };
    let expected_terminal_deadline =
        account_cooldown_deadline(terminal_preimage).expect("terminal cooldown");
    assert!(matches!(
        &events[1],
        TombstoneStoreEvent::Store {
            capability,
            phase: UsageRefreshPhase::Idle,
            result_free: true,
            cooldown_deadline_epoch: Some(deadline),
            generation: _,
        } if capability == &terminal_preimage.capability
            && *deadline == expected_terminal_deadline
    ));
    assert_eq!(
        events[2],
        TombstoneStoreEvent::Purge(later_purge_preimage.capability.clone())
    );
    assert_eq!(
        events[3],
        TombstoneStoreEvent::Store {
            capability: pending_preimage.capability.clone(),
            phase: UsageRefreshPhase::Updating,
            generation: if pending_preimage_missing {
                marker_generation
            } else {
                pending_preimage.generation
            },
            result_free: true,
            cooldown_deadline_epoch: account_cooldown_deadline(restored_pending),
        }
    );
    assert_eq!(
        events[4],
        TombstoneStoreEvent::Store {
            capability: terminal_preimage.capability.clone(),
            phase: terminal_preimage.phase,
            generation: terminal_preimage.generation,
            result_free: false,
            cooldown_deadline_epoch: account_cooldown_deadline(terminal_preimage),
        }
    );
    assert_eq!(
        events[5],
        TombstoneStoreEvent::Store {
            capability: later_purge_preimage.capability.clone(),
            phase: later_purge_preimage.phase,
            generation: later_purge_preimage.generation,
            result_free: true,
            cooldown_deadline_epoch: account_cooldown_deadline(later_purge_preimage),
        }
    );
}

fn assert_missing_pending_restart_fence(
    pending: &UsageAccountCapability,
    restored_pending: &AccountStateEnvelope,
    clock: &Arc<FakeMonotonicClock>,
) {
    let restart_store = Arc::new(MemoryStore::default());
    restart_store.store(restored_pending, 1_001).unwrap();
    let restart_executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_001, 80),
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce restart fixture ports to coordinator trait objects"
    )]
    let restart_provider: Arc<dyn UsageProviderExecutor> = restart_executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce restart fixture ports to coordinator trait objects"
    )]
    let restart_state_store: Arc<dyn AccountStateStore> = restart_store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "share fake clock with the restart fixture"
    )]
    let restart_clock: Arc<dyn MonotonicClock> = clock.clone();
    let restarted = UsageCoordinator::start_with_clock(
        restart_provider,
        restart_state_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([(pending.clone(), "revision-a".into())])),
        None,
        restart_clock,
    );
    let recovered = restarted.current(pending, 1_001).unwrap();
    assert_eq!(recovered.phase, UsageRefreshPhase::Failed);
    assert_eq!(recovered.retry_at_epoch, Some(1_300));
    let early = restarted
        .request_refresh(pending, recovered.generation, true, 1_001)
        .unwrap();
    assert_eq!(early.generation, recovered.generation);
    assert_eq!(restart_executor.calls.load(Ordering::SeqCst), 0);
}

fn assert_pending_join_and_finish(
    coordinator: &UsageCoordinator,
    executor: &GateExecutor,
    pending: &UsageAccountCapability,
    active_generation: u64,
) {
    let current = coordinator.current(pending, 1_001).unwrap();
    assert_eq!(current.phase, UsageRefreshPhase::Updating);
    let joiner = coordinator
        .request_refresh(pending, current.generation, true, 1_001)
        .unwrap();
    assert_eq!(joiner.generation, active_generation);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    executor.release(1);
    executor.wait_idle();
    assert_eq!(
        join_ok(coordinator, pending, active_generation, 1_002).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

fn failed_later_purge_case(
    revision_reset: bool,
    pending_preimage_missing: bool,
    terminal_retry_after: bool,
) {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(TombstoneWriteFailureStore {
        inner: MemoryStore::default(),
        fail_tombstone: AtomicUsize::new(0),
        fail_purge_after_removal: AtomicUsize::new(0),
        events: Mutex::new(Vec::new()),
    });
    let pending = capability("a-pending-account");
    let terminal = capability("m-terminal-cooldown-account");
    let later_purge = UsageAccountCapability {
        account_id: "z-later-purge-account".into(),
        surface_id: "codex".into(),
    };
    let terminal_preimage = if terminal_retry_after {
        let mut envelope = completed_claude_attempt(&terminal, 900, 1_300);
        envelope.phase = UsageRefreshPhase::Failed;
        envelope.terminal_result = None;
        envelope.success_deadline_epoch = None;
        envelope.terminal_error = Some(coordination_error(
            UsageCoordinationErrorKind::RateLimited,
            "provider rate limited",
        ));
        envelope.rate_limit_deadline_epoch = Some(1_700);
        envelope.retry_deadline_epoch = Some(1_700);
        envelope
    } else {
        completed_claude_attempt(&terminal, 900, 1_300)
    };
    let later_purge_preimage = AccountStateEnvelope::idle(later_purge.clone());
    store.inner.store(&terminal_preimage, 1_001).unwrap();
    store.inner.store(&later_purge_preimage, 1_001).unwrap();
    let clock = Arc::new(FakeMonotonicClock::with_wall_epoch(Duration::from_secs(
        1_000,
    )));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let provider: Arc<dyn UsageProviderExecutor> = executor.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce fixture ports to coordinator trait objects"
    )]
    let state_store: Arc<dyn AccountStateStore> = store.clone();
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce paired fake clock to the coordinator clock port"
    )]
    let clock_port: Arc<dyn MonotonicClock> = clock.clone();
    let coordinator = UsageCoordinator::start_with_clock(
        provider,
        state_store,
        UsageCoordinatorConfig::default(),
        Some(BTreeMap::from([
            (pending.clone(), "revision-a".into()),
            (terminal.clone(), "revision-a".into()),
            (later_purge.clone(), "revision-a".into()),
        ])),
        None,
        clock_port,
    );

    let active = coordinator
        .request_refresh(&pending, 0, true, 1_001)
        .unwrap();
    executor.wait_started(1);
    let pending_preimage = store.inner.load(&pending, 1_001).unwrap().unwrap();
    assert_eq!(pending_preimage.phase, UsageRefreshPhase::Updating);
    assert_eq!(
        store.inner.load(&terminal, 1_001).unwrap(),
        Some(terminal_preimage.clone())
    );
    if pending_preimage_missing {
        assert!(
            store
                .inner
                .states
                .lock()
                .unwrap()
                .remove(&pending)
                .is_some()
        );
    }

    store.events.lock().unwrap().clear();
    store.fail_purge_after_removal.store(1, Ordering::SeqCst);
    let next_catalog = if revision_reset {
        vec![catalog_entry(&pending, "revision-b")]
    } else {
        Vec::new()
    };
    let error = coordinator
        .reconcile_catalog(next_catalog, 1_001)
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    let restored_pending = store
        .inner
        .load(&pending, 1_001)
        .unwrap()
        .expect("pending attempt marker must survive rollback");
    assert_eq!(restored_pending.phase, UsageRefreshPhase::Updating);
    assert!(restored_pending.terminal_result.is_none());
    assert!(restored_pending.last_good.is_none());
    assert!(restored_pending.terminal_error.is_none());
    if pending_preimage_missing {
        assert!(restored_pending.generation > pending_preimage.generation);
    } else {
        assert_eq!(restored_pending, pending_preimage.clone());
    }
    assert_eq!(
        store.inner.load(&terminal, 1_001).unwrap(),
        Some(terminal_preimage.clone())
    );
    assert_eq!(
        store.inner.load(&later_purge, 1_001).unwrap(),
        Some(later_purge_preimage.clone())
    );

    assert_later_purge_store_order(
        &store,
        &pending_preimage,
        &restored_pending,
        pending_preimage_missing,
        &terminal_preimage,
        &later_purge_preimage,
    );

    if pending_preimage_missing {
        assert_missing_pending_restart_fence(&pending, &restored_pending, &clock);
    }
    assert_pending_join_and_finish(&coordinator, &executor, &pending, active.generation);
}

#[test]
fn failed_later_purge_prewrites_lazy_success_tombstone_before_purge() {
    failed_later_purge_case(false, false, false);
}

#[test]
fn failed_later_purge_during_revision_reset_restores_pending_marker() {
    failed_later_purge_case(true, false, false);
}

#[test]
fn failed_later_purge_with_missing_pending_preimage_preserves_restart_fence() {
    failed_later_purge_case(false, true, false);
}

#[test]
fn failed_later_purge_prewrites_lazy_retry_after_tombstone_before_purge() {
    failed_later_purge_case(false, false, true);
}

struct CapabilityRecordingExecutor {
    calls: Mutex<Vec<UsageAccountCapability>>,
}

impl UsageProviderExecutor for CapabilityRecordingExecutor {
    fn probe(&self, capability: &UsageAccountCapability, _generation: u64) -> ProviderProbeOutcome {
        self.calls.lock().unwrap().push(capability.clone());
        ProviderProbeOutcome::success(quota_view(1_000, 80))
    }
}

#[test]
fn selected_cadence_polls_only_the_opted_in_capability() {
    let executor = Arc::new(CapabilityRecordingExecutor {
        calls: Mutex::new(Vec::new()),
    });
    let store = Arc::new(MemoryStore::default());
    let opted_in = capability("opted-in-account");
    let stopped = capability("stopped-account");
    let coordinator = UsageCoordinator::with_catalog(
        Arc::<CapabilityRecordingExecutor>::clone(&executor),
        Arc::<MemoryStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        [
            catalog_entry(&opted_in, "revision-a"),
            catalog_entry(&stopped, "revision-a"),
        ],
    );
    assert_eq!(coordinator.current(&opted_in, 1_000).unwrap().generation, 0);
    assert_eq!(coordinator.current(&stopped, 1_000).unwrap().generation, 0);

    let first = coordinator.poll_due_for_capabilities([opted_in.clone()], 1_000);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].capability, opted_in);
    drop(join_ok(&coordinator, &opted_in, first[0].generation, 1_001));

    let selected_due = coordinator
        .next_due_epoch_for_capabilities([opted_in.clone()], 1_001)
        .unwrap();
    assert!(selected_due > 1_000);
    assert_eq!(
        coordinator.next_due_epoch(),
        Some(1_000),
        "the global due time still belongs to the omitted account"
    );
    let second = coordinator.poll_due_for_capabilities([opted_in.clone()], selected_due);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].capability, opted_in);
    drop(join_ok(
        &coordinator,
        &opted_in,
        second[0].generation,
        selected_due + 1,
    ));
    assert_eq!(
        *executor.calls.lock().unwrap(),
        vec![opted_in.clone(), opted_in.clone()]
    );
    assert_eq!(coordinator.current(&stopped, 1_001).unwrap().generation, 0);
}
