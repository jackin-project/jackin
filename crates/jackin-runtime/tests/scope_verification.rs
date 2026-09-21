// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! S2/S6 scenario proof through the public relay/coordinator boundary.
//!
//! S2 (relay layer): a W1 container allowlist admits exactly A/B/C. Forged
//! IDs (unknown account, same-surface sibling), stale catalog capabilities,
//! and empty scopes are all denied without provider work.
//! S6: refresh ownership races resolve deterministically — simultaneous
//! refreshes single-flight onto one generation, rotation fences in-flight
//! work, removal mid-request revokes it, and stale compare-and-swap observes
//! adopt the winner instead of queueing duplicate probes.

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationErrorKind, UsageRefreshPhase,
};
use jackin_usage::coordinator::{
    AccountStateEnvelope, AccountStateStore, ProviderProbeOutcome, StateStoreError,
    UsageCoordinator, UsageCoordinatorConfig, UsageProviderExecutor,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

fn capability(account_id: &str) -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: account_id.into(),
        surface_id: "claude".into(),
    }
}

fn entry(account_id: &str, revision: &str) -> UsageCatalogEntry {
    UsageCatalogEntry {
        capability: capability(account_id),
        revision: revision.into(),
    }
}

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

    fn purge(&self, capability: &UsageAccountCapability) -> Result<(), StateStoreError> {
        self.states.lock().unwrap().remove(capability);
        Ok(())
    }
}

/// Deterministic provider gate: probes block until the test releases them,
/// so every race below synchronizes on condvars instead of sleeps.
struct GateExecutor {
    calls: AtomicUsize,
    active: AtomicUsize,
    started: (Mutex<usize>, Condvar),
    permits: (Mutex<usize>, Condvar),
    outcome: Mutex<ProviderProbeOutcome>,
}

impl GateExecutor {
    fn new(outcome: ProviderProbeOutcome) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            started: (Mutex::new(0), Condvar::new()),
            permits: (Mutex::new(0), Condvar::new()),
            outcome: Mutex::new(outcome),
        }
    }

    fn wait_idle(&self) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while self.active.load(Ordering::SeqCst) != 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "provider probe did not finish"
            );
            std::thread::yield_now();
        }
    }

    fn wait_started(&self, expected: usize) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let (lock, changed) = &self.started;
        let mut started = lock.lock().unwrap();
        while *started < expected {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
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
}

impl UsageProviderExecutor for GateExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.active.fetch_add(1, Ordering::SeqCst);
        let (started_lock, started_changed) = &self.started;
        *started_lock.lock().unwrap() += 1;
        started_changed.notify_all();
        let (permit_lock, permit_changed) = &self.permits;
        let mut permits = permit_lock.lock().unwrap();
        while *permits == 0 {
            let (next, wait) = permit_changed
                .wait_timeout(permits, Duration::from_secs(5))
                .unwrap();
            permits = next;
            assert!(!wait.timed_out(), "provider probe permit was not released");
        }
        *permits -= 1;
        self.active.fetch_sub(1, Ordering::SeqCst);
        self.outcome.lock().unwrap().clone()
    }
}

fn quota_view() -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("claude", 1_700_000_000);
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.account.provider_label = "Claude".to_owned();
    view.account.account_label = "scope-proof@example.test".to_owned();
    view.buckets = vec![QuotaBucketView {
        label: "Weekly".to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(55),
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }];
    view
}

fn coordinator_with(executor: Arc<GateExecutor>) -> (UsageCoordinator, Arc<GateExecutor>) {
    let provider: Arc<dyn UsageProviderExecutor> = executor.clone();
    let store: Arc<dyn AccountStateStore> = Arc::new(MemoryStore::default());
    let coordinator = UsageCoordinator::new(provider, store, UsageCoordinatorConfig::default());
    (coordinator, executor)
}

const NOW: i64 = 1_800_000_000;
const WAIT: Duration = Duration::from_secs(5);

/// S2: the W1 relay allowlist admits exactly A/B/C and nothing else.
#[test]
fn s2_relay_allowlist_admits_exactly_abc() {
    use jackin_usage::coordinator::UsageCapabilitySet;

    let allowlist = UsageCapabilitySet::new([
        capability("acc-a"),
        capability("acc-b"),
        capability("acc-c"),
    ]);
    assert_eq!(allowlist.len(), 3);
    for id in ["acc-a", "acc-b", "acc-c"] {
        assert!(
            allowlist.authorize(&capability(id)).is_ok(),
            "{id} must be admitted"
        );
    }
    // Forged D: same shape, same surface, never launched here.
    let err = allowlist.authorize(&capability("acc-d")).unwrap_err();
    assert_eq!(err.kind, UsageCoordinationErrorKind::Unauthorized);
    // Forged surface sibling: same account id, different surface.
    let sibling = UsageAccountCapability {
        account_id: "acc-a".into(),
        surface_id: "codex".into(),
    };
    let err = allowlist.authorize(&sibling).unwrap_err();
    assert_eq!(err.kind, UsageCoordinationErrorKind::Unauthorized);
    // Surface resolution is exact: the shared surface is ambiguous across
    // three accounts, so it denies rather than guessing.
    let err = allowlist.resolve_surface("claude").unwrap_err();
    assert_eq!(err.kind, UsageCoordinationErrorKind::Unauthorized);
    let err = allowlist.resolve_surface("no-such-surface").unwrap_err();
    assert_eq!(err.kind, UsageCoordinationErrorKind::Unauthorized);
}

/// S2: an empty scope denies every capability, including well-formed ones.
#[test]
fn s2_empty_scope_denies_everything() {
    use jackin_usage::coordinator::UsageCapabilitySet;

    let allowlist = UsageCapabilitySet::new([]);
    assert!(allowlist.is_empty());
    for id in ["acc-a", "acc-b", "acc-c", "acc-d"] {
        let err = allowlist.authorize(&capability(id)).unwrap_err();
        assert_eq!(
            err.kind,
            UsageCoordinationErrorKind::Unauthorized,
            "{id} leaked"
        );
    }
    let err = allowlist.resolve_surface("claude").unwrap_err();
    assert_eq!(err.kind, UsageCoordinationErrorKind::Unauthorized);
}

/// S2: a capability removed from the broker catalog is stale: direct backend
/// refresh and join requests fail with `CatalogRevoked`, never state.
#[test]
fn s2_stale_catalog_capability_is_revoked_for_direct_requests() {
    let gate = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(),
    )));
    let (coordinator, gate) = coordinator_with(gate);
    coordinator
        .reconcile_catalog([entry("acc-a", "rev-1"), entry("acc-d", "rev-1")], NOW)
        .unwrap();

    // D is live before removal; wait until its probe is in flight so the
    // revocation below lands strictly mid-request.
    let live = coordinator
        .request_refresh(&capability("acc-d"), 0, true, NOW)
        .unwrap();
    assert_eq!(live.phase, UsageRefreshPhase::Queued);
    gate.wait_started(1);

    // Removal mid-request revokes D immediately.
    coordinator
        .reconcile_catalog([entry("acc-a", "rev-1")], NOW)
        .unwrap();
    let err = coordinator
        .join_generation(&capability("acc-d"), live.generation, WAIT, NOW)
        .unwrap_err();
    assert_eq!(err.kind, UsageCoordinationErrorKind::CatalogRevoked);
    let err = coordinator
        .request_refresh(&capability("acc-d"), 0, true, NOW)
        .unwrap_err();
    assert_eq!(err.kind, UsageCoordinationErrorKind::CatalogRevoked);
    // The revoked probe's late result is discarded, not materialized.
    gate.release(1);
    gate.wait_idle();

    // A still-catalogued sibling is unaffected and completes normally.
    let sibling = coordinator
        .request_refresh(&capability("acc-a"), 0, true, NOW)
        .unwrap();
    assert_eq!(sibling.phase, UsageRefreshPhase::Queued);
    gate.wait_started(2);
    gate.release(1);
    let done = coordinator
        .join_generation(&capability("acc-a"), sibling.generation, WAIT, NOW)
        .unwrap();
    assert_eq!(done.phase, UsageRefreshPhase::Completed);
}

/// S6: simultaneous refreshes single-flight onto one generation; one probe serves all joiners.
#[test]
fn s6_simultaneous_refreshes_share_one_generation() {
    let gate = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(),
    )));
    let (coordinator, gate) = coordinator_with(gate);
    let coordinator = Arc::new(coordinator);
    let cap = capability("acc-a");

    // Eight racing refreshes against a closed gate: all must observe the same winner.
    let mut handles = Vec::new();
    for _ in 0..8 {
        let coordinator = Arc::clone(&coordinator);
        let cap = cap.clone();
        handles.push(std::thread::spawn(move || {
            coordinator.request_refresh(&cap, 0, true, NOW)
        }));
    }
    let mut generations = Vec::new();
    for handle in handles {
        let view = handle.join().unwrap().unwrap();
        generations.push(view.generation);
    }
    gate.wait_started(1);
    assert!(
        generations.iter().all(|g| *g == generations[0]),
        "races split generations: {generations:?}"
    );
    gate.release(1);
    let terminal = coordinator
        .join_generation(&cap, generations[0], WAIT, NOW)
        .unwrap();
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert_eq!(gate.calls.load(Ordering::SeqCst), 1, "duplicate probes ran");
}

/// S6: a stale compare-and-swap observe adopts the in-flight winner and never
/// queues a second force refresh.
#[test]
fn s6_stale_cas_adopts_the_winner() {
    let gate = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(),
    )));
    let (coordinator, gate) = coordinator_with(gate);
    let cap = capability("acc-a");

    let winner = coordinator.request_refresh(&cap, 0, true, NOW).unwrap();
    assert_eq!(winner.phase, UsageRefreshPhase::Queued);
    // A second force refresh with a stale observed generation joins, not forks.
    let joiner = coordinator.request_refresh(&cap, 0, true, NOW).unwrap();
    assert_eq!(joiner.generation, winner.generation);
    gate.wait_started(1);
    gate.release(1);
    let terminal = coordinator
        .join_generation(&cap, winner.generation, WAIT, NOW)
        .unwrap();
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert_eq!(gate.calls.load(Ordering::SeqCst), 1);

    // Generations stay CAS-addressed after completion: joining the old
    // generation returns its own terminal view, never the newer one.
    let next = coordinator
        .request_refresh(&cap, winner.generation, true, NOW)
        .unwrap();
    assert_eq!(next.generation, winner.generation + 1);
    gate.wait_started(2);
    gate.release(1);
    let terminal_next = coordinator
        .join_generation(&cap, next.generation, WAIT, NOW)
        .unwrap();
    assert_eq!(terminal_next.phase, UsageRefreshPhase::Completed);
    let replay = coordinator
        .join_generation(&cap, winner.generation, WAIT, NOW)
        .unwrap();
    assert_eq!(replay.generation, winner.generation);
    assert_eq!(replay.phase, UsageRefreshPhase::Completed);
}

/// S6: catalog rotation fences the in-flight generation immediately; its late
/// result is discarded and the next refresh starts clean under the new revision.
#[test]
fn s6_rotation_fences_in_flight_and_discards_late_results() {
    let gate = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(),
    )));
    let (coordinator, gate) = coordinator_with(gate);
    let cap = capability("acc-a");
    coordinator
        .reconcile_catalog([entry("acc-a", "rev-1")], NOW)
        .unwrap();

    let stale = coordinator.request_refresh(&cap, 0, true, NOW).unwrap();
    gate.wait_started(1);
    coordinator
        .reconcile_catalog([entry("acc-a", "rev-2")], NOW)
        .unwrap();

    // The join fails immediately even though the probe is still running.
    let err = coordinator
        .join_generation(&cap, stale.generation, WAIT, NOW)
        .unwrap_err();
    assert_eq!(err.kind, UsageCoordinationErrorKind::CatalogRevoked);

    // The late probe result lands on a fenced generation and is discarded.
    gate.release(1);
    gate.wait_idle();
    // Rotation reset the cursor to Idle; observing the stale generation would
    // adopt, not fork — so a correct CAS observes the current generation.
    let cursor = coordinator.current(&cap, NOW).unwrap();
    assert_eq!(cursor.phase, UsageRefreshPhase::Idle);
    assert!(cursor.snapshot.is_none(), "rotation must drop last-good");
    let fresh = coordinator
        .request_refresh(&cap, cursor.generation, true, NOW)
        .unwrap();
    assert_eq!(fresh.generation, stale.generation + 2);
    assert_eq!(fresh.phase, UsageRefreshPhase::Queued);
    gate.wait_started(2);
    gate.release(1);
    let terminal = coordinator
        .join_generation(&cap, fresh.generation, WAIT, NOW)
        .unwrap();
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert!(
        terminal.snapshot.is_some(),
        "fresh generation lost its data"
    );
    assert_eq!(gate.calls.load(Ordering::SeqCst), 2);
}

/// S6: distinct accounts refresh concurrently within the bound; one account's
/// gate never blocks another's completion.
#[test]
fn s6_distinct_accounts_refresh_independently() {
    let gate = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(),
    )));
    let (coordinator, gate) = coordinator_with(gate);
    let a = capability("acc-a");
    let b = capability("acc-b");

    let gen_a = coordinator.request_refresh(&a, 0, true, NOW).unwrap();
    let gen_b = coordinator.request_refresh(&b, 0, true, NOW).unwrap();
    gate.wait_started(2);
    gate.release(2);
    let done_a = coordinator
        .join_generation(&a, gen_a.generation, WAIT, NOW)
        .unwrap();
    let done_b = coordinator
        .join_generation(&b, gen_b.generation, WAIT, NOW)
        .unwrap();
    assert_eq!(done_a.phase, UsageRefreshPhase::Completed);
    assert_eq!(done_b.phase, UsageRefreshPhase::Completed);
    assert_eq!(done_a.capability.account_id, "acc-a");
    assert_eq!(done_b.capability.account_id, "acc-b");
    assert_eq!(gate.calls.load(Ordering::SeqCst), 2);
}
