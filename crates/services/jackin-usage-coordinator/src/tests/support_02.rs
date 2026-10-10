// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
impl CadenceGateExecutor {
    pub(super) fn new(outcome: ProviderProbeOutcome) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            started: (Mutex::new(0), Condvar::new()),
            permits: (Mutex::new(0), Condvar::new()),
            outcome: Mutex::new(outcome),
        }
    }

    pub(super) fn wait_started(&self) {
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

    pub(super) fn release(&self) {
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

pub(super) fn cadence_quota_view(epoch: i64) -> FocusedUsageView {
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

pub(super) fn cadence_coordinator<E>(
    executor: Arc<E>,
    config: UsageCoordinatorConfig,
) -> UsageCoordinator
where
    E: UsageProviderExecutor + 'static,
{
    let executor: Arc<dyn UsageProviderExecutor> = executor;
    let store: Arc<dyn AccountStateStore> = Arc::new(CadenceMemoryStore {
        states: Mutex::new(BTreeMap::new()),
    });
    coordinator_with_fake_clock(executor, store, config)
}
