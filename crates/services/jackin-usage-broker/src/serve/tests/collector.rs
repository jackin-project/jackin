// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, StatusSlot, UsageConfidence, UsageSeverity,
    UsageSnapshotStatus, UsageSource,
};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageFreshnessPhaseV1,
    UsageProjectionRefreshStateV1, UsageRefreshPhase,
};
use jackin_protocol::usage_monitor::{
    MonitorAccountBindingInput, MonitorConfig, MonitorEvidenceSource, MonitorOperation,
    MonitorProvider, MonitorPurpose, MonitorReply, MonitorScope,
};
use jackin_usage_coordinator::{
    ClockSample as CoordinatorClockSample, FileAccountStateStore, FileProjectionStateStore,
    MonotonicClock, ProviderProbeOutcome, UsageCoordinator, UsageCoordinatorConfig,
    UsageProviderExecutor,
};

const COLLECTOR_ACCOUNT_ID: &str = "collector-test-account";
const SOURCE_ID_PREFIX: &str = "a";

struct CollectorCoordinatorClock {
    sample: Mutex<ClockSample>,
}

impl CollectorCoordinatorClock {
    fn at(wall_epoch: i64) -> Self {
        Self {
            sample: Mutex::new(ClockSample {
                wall_epoch,
                monotonic_elapsed: Duration::ZERO,
            }),
        }
    }

    fn set(&self, sample: ClockSample) {
        let mut current = self.sample.lock().expect("collector test clock");
        assert!(
            sample.wall_epoch >= current.wall_epoch,
            "collector test wall clock cannot move backward"
        );
        assert!(
            sample.monotonic_elapsed >= current.monotonic_elapsed,
            "collector test monotonic clock cannot move backward"
        );
        *current = sample;
    }
}

impl MonotonicClock for CollectorCoordinatorClock {
    fn now(&self) -> Duration {
        self.sample
            .lock()
            .expect("collector test clock")
            .monotonic_elapsed
    }

    fn sample(&self, _fallback_epoch: i64) -> CoordinatorClockSample {
        let sample = self.sample.lock().expect("collector test clock");
        CoordinatorClockSample::anchored(sample.wall_epoch, sample.monotonic_elapsed)
    }
}

fn source_id() -> String {
    SOURCE_ID_PREFIX.repeat(64)
}

struct GatedExecutor {
    started: Sender<()>,
    release: Mutex<Receiver<()>>,
    calls: AtomicUsize,
}

impl UsageProviderExecutor for GatedExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let _ignored = self.started.send(());
        if let Ok(release) = self.release.lock() {
            let _ignored = release.recv();
        }
        ProviderProbeOutcome::success(provider_view(NOW + 2))
    }
}

struct RetryAfterExecutor {
    calls: AtomicUsize,
}

impl UsageProviderExecutor for RetryAfterExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        match self.calls.fetch_add(1, Ordering::SeqCst) + 1 {
            1 => ProviderProbeOutcome::success(provider_view(NOW)),
            2 => ProviderProbeOutcome::Failure {
                kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::RateLimited,
                message: "fixture retry after".to_owned(),
                retry_at_epoch: Some(NOW + 1_200),
            },
            _ => ProviderProbeOutcome::success(provider_view(NOW + 1_200)),
        }
    }
}

fn provider_view(now_epoch: i64) -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("fixture", now_epoch);
    view.focused_agent = Some("claude".to_owned());
    view.focused_provider = Some("Claude".to_owned());
    view.account.provider_label = "Anthropic".to_owned();
    view.account.account_label = "fixture-account".to_owned();
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.buckets = [
        ("Session", StatusSlot::Session, 64, now_epoch + 3_600),
        (
            "Weekly",
            StatusSlot::Weekly,
            72,
            now_epoch + 7 * 24 * 60 * 60,
        ),
    ]
    .into_iter()
    .map(
        |(label, status_slot, remaining, resets_at)| QuotaBucketView {
            label: label.to_owned(),
            used_label: None,
            limit_label: None,
            remaining_percent: Some(remaining),
            reset_label: None,
            resets_at: Some(resets_at),
            status_slot: Some(status_slot),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        },
    )
    .collect();
    view.last_error = None;
    view
}

struct CollectorHarness {
    store: Arc<MonitorStore>,
    coordinator: Arc<UsageCoordinator>,
    coordinator_clock: Arc<CollectorCoordinatorClock>,
    publisher: publish::ProjectionPublisher,
    temp: tempfile::TempDir,
    capability: UsageAccountCapability,
}

impl CollectorHarness {
    fn new(executor: Arc<dyn UsageProviderExecutor>) -> Self {
        let temp = tempfile::tempdir().expect("temporary collector state");
        let source_id = source_id();
        let capability = crate::service::claude_usage_capability_for_source_id(&source_id);
        let catalog = vec![UsageCatalogEntry {
            revision: "collector-test-catalog-revision".to_owned(),
            capability: capability.clone(),
        }];
        let coordinator_clock = Arc::new(CollectorCoordinatorClock::at(NOW));
        let coordinator_clock_for_coordinator: Arc<dyn MonotonicClock> = coordinator_clock.clone();
        let coordinator = Arc::new(UsageCoordinator::with_catalog_and_clock(
            executor,
            Arc::new(FileAccountStateStore::at(temp.path().join("accounts"))),
            UsageCoordinatorConfig::default(),
            catalog.clone(),
            coordinator_clock_for_coordinator,
        ));
        let publisher = publish::ProjectionPublisher::new(
            Arc::clone(&coordinator),
            Arc::new(Mutex::new(crate::projection::empty_projection(
                "collector-test",
            ))),
            FileProjectionStateStore::under_data_dir(temp.path()),
        )
        .with_catalog(catalog);
        let store = Arc::new(MonitorStore::open(temp.path()).expect("open monitor state"));
        store.set_experimental_collector_source(Some(source_id));
        Self {
            store,
            coordinator,
            coordinator_clock,
            publisher,
            temp,
            capability,
        }
    }

    fn set_coordinator_clock(&self, sample: ClockSample) {
        self.coordinator_clock.set(sample);
    }

    fn start_approved_observer(&self) -> String {
        let source_id = source_id();
        let binding = match self
            .store
            .operate(
                MonitorOperation::BindAccount {
                    binding: MonitorAccountBindingInput {
                        provider: MonitorProvider::Claude,
                        account_id: COLLECTOR_ACCOUNT_ID.to_owned(),
                        operator_label: "collector-test-operator".to_owned(),
                        operator_confirmed: true,
                        provider_account_id: Some(source_id.clone()),
                        experimental_collector_approved: true,
                    },
                },
                NOW,
            )
            .expect("bind explicitly approved source capability")
        {
            MonitorReply::AccountBound { binding } => binding,
            other => panic!("expected account-bound reply, got {other:?}"),
        };
        assert!(
            self.store.collection_accounts().is_empty(),
            "a confirmed binding alone cannot enter collector polling"
        );

        let reply = self
            .store
            .operate(
                MonitorOperation::Start {
                    config: MonitorConfig {
                        provider: MonitorProvider::Claude,
                        purpose: MonitorPurpose::ObserveOnly,
                        scope: MonitorScope::BoundAccount {
                            binding_id: binding.binding_id,
                            binding_revision: binding.revision,
                            session_id: None,
                        },
                        goal_id: None,
                        expected_model: None,
                        policy_revision: None,
                        experimental_collector: true,
                    },
                    idempotency_key: "approved-collector-observer".to_owned(),
                },
                NOW,
            )
            .expect("start opted-in observer");
        let MonitorReply::Started { status } = reply else {
            panic!("expected collector observer to start, got {reply:?}");
        };
        assert_eq!(
            self.store.collection_accounts(),
            vec![source_id],
            "only the exact approved foreground source with an active opted-in monitor enters collector polling"
        );
        status.monitor_id
    }

    fn ticker_state(&self) -> TickerState {
        TickerState {
            last_sample: ClockSample {
                wall_epoch: NOW,
                monotonic_elapsed: Duration::ZERO,
            },
            last_observed_projection_id: self
                .publisher
                .current_projection()
                .expect("read initial projection")
                .projection_id
                .into(),
            last_ticked_wake: None,
            retry: None,
            last_collection_check: None,
            last_publish_attempt: None,
            observation_retry_after: None,
        }
    }

    fn monitor_state_file(&self) -> std::path::PathBuf {
        self.temp
            .path()
            .join(crate::BROKER_DIR)
            .join("monitor")
            .join("state.json")
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "One fake-clock regression verifies terminal projection publication and monitor persistence retry in the collector path."
)]
fn ticker_polls_approved_source_and_publishes_terminal_result_while_idle() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::sync_channel(0);
    let executor = Arc::new(GatedExecutor {
        started: started_sender,
        release: Mutex::new(release_receiver),
        calls: AtomicUsize::new(0),
    });
    let executor_trait: Arc<dyn UsageProviderExecutor> = Arc::<GatedExecutor>::clone(&executor);
    let harness = CollectorHarness::new(executor_trait);
    let monitor_id = harness.start_approved_observer();
    let mut ticker_state = harness.ticker_state();
    let initial_sample = ClockSample {
        wall_epoch: NOW,
        monotonic_elapsed: Duration::ZERO,
    };
    harness.set_coordinator_clock(initial_sample);

    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        initial_sample,
        &mut ticker_state,
    );
    started_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("approved observer dispatches through the ticker");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let generation = harness
        .coordinator
        .current(&harness.capability, NOW)
        .expect("load active collector generation")
        .generation;
    let updating = harness
        .publisher
        .current_projection()
        .expect("read updating projection");
    assert_eq!(
        updating.refresh_state,
        UsageProjectionRefreshStateV1::Refreshing
    );

    let state_file = harness.monitor_state_file();
    let backup_file = state_file.with_extension("state-backup");
    fs::rename(&state_file, &backup_file).expect("move state file for persistence fault");
    symlink(&backup_file, &state_file).expect("install test-only state symlink");
    harness.set_coordinator_clock(ClockSample {
        wall_epoch: NOW + 2,
        monotonic_elapsed: Duration::from_secs(2),
    });
    release_sender.send(()).expect("release provider result");
    let terminal = harness
        .coordinator
        .join_generation(
            &harness.capability,
            generation,
            Duration::from_secs(1),
            NOW + 2,
        )
        .expect("wait for terminal provider result");
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert!(harness.coordinator.is_idle());

    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 2,
            monotonic_elapsed: Duration::from_secs(2),
        },
        &mut ticker_state,
    );
    let completed = harness
        .publisher
        .current_projection()
        .expect("terminal state publishes without a client refresh");
    assert_eq!(completed.refresh_state, UsageProjectionRefreshStateV1::Idle);
    let account = completed
        .providers
        .iter()
        .flat_map(|provider| &provider.accounts)
        .find(|account| account.canonical_account_id == harness.capability.account_id)
        .expect("hashed capability is published");
    assert_eq!(account.freshness.phase, UsageFreshnessPhaseV1::Current);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        ticker_state.last_observed_projection_id.as_deref(),
        Some(updating.projection_id.as_str()),
        "failed monitor persistence must leave the terminal projection pending"
    );
    let retry_after = ticker_state.observation_retry_after;
    // Stay below the one-second monotonic persistence retry while wall time
    // remains in the same whole second.
    let retry_sample = ClockSample {
        wall_epoch: NOW + 2,
        monotonic_elapsed: Duration::from_millis(2_200),
    };
    harness.set_coordinator_clock(retry_sample);
    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        retry_sample,
        &mut ticker_state,
    );
    assert_eq!(ticker_state.observation_retry_after, retry_after);

    fs::remove_file(&state_file).expect("remove test-only symlink");
    fs::rename(&backup_file, &state_file).expect("restore monitor state file");
    let restored_sample = ClockSample {
        wall_epoch: NOW + 3,
        monotonic_elapsed: Duration::from_millis(3_200),
    };
    harness.set_coordinator_clock(restored_sample);
    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        restored_sample,
        &mut ticker_state,
    );
    assert_eq!(
        ticker_state.last_observed_projection_id.as_deref(),
        Some(completed.projection_id.as_str()),
        "successful persistence records the completed projection"
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let MonitorReply::Status { status } = harness
        .store
        .operate(MonitorOperation::Status { monitor_id }, NOW + 3)
        .expect("read monitor evidence after terminal publication")
    else {
        panic!("expected monitor status after terminal publication");
    };
    assert_eq!(
        status.five_hour.used_percentage_basis_points,
        Some(3_600),
        "monitor status after paired-clock collection: {status:#?}"
    );
    assert_eq!(status.seven_day.used_percentage_basis_points, Some(2_800));
    let evidence_source = |sequence| {
        status
            .evidence
            .iter()
            .find(|evidence| evidence.sequence == sequence)
            .map(|evidence| evidence.source)
    };
    assert_eq!(
        evidence_source(
            status
                .five_hour
                .used_evidence
                .as_ref()
                .expect("five-hour broker evidence")
                .evidence_sequence
        ),
        Some(MonitorEvidenceSource::BrokerProjection)
    );
    assert_eq!(
        evidence_source(
            status
                .seven_day
                .used_evidence
                .as_ref()
                .expect("seven-day broker evidence")
                .evidence_sequence
        ),
        Some(MonitorEvidenceSource::BrokerProjection)
    );
}

#[test]
fn ticker_collector_preserves_minimum_attempt_floor_and_retry_after() {
    let executor = Arc::new(RetryAfterExecutor {
        calls: AtomicUsize::new(0),
    });
    let executor_trait: Arc<dyn UsageProviderExecutor> =
        Arc::<RetryAfterExecutor>::clone(&executor);
    let harness = CollectorHarness::new(executor_trait);
    harness.start_approved_observer();
    let mut ticker_state = harness.ticker_state();

    let tick = |wall_epoch, monotonic_elapsed, state: &mut TickerState| {
        let sample = ClockSample {
            wall_epoch,
            monotonic_elapsed,
        };
        harness.set_coordinator_clock(sample);
        publisher_tick_step(
            &harness.publisher,
            &harness.coordinator,
            &harness.store,
            sample,
            state,
        );
    };
    tick(NOW, Duration::ZERO, &mut ticker_state);
    let first_generation = harness
        .coordinator
        .current(&harness.capability, NOW)
        .expect("first collector generation")
        .generation;
    harness
        .coordinator
        .join_generation(
            &harness.capability,
            first_generation,
            Duration::from_secs(1),
            NOW,
        )
        .expect("join initial successful provider result");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    tick(NOW + 299, Duration::from_secs(299), &mut ticker_state);
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        1,
        "a successful Claude result retains the 300-second attempt floor"
    );
    tick(NOW + 300, Duration::from_secs(300), &mut ticker_state);
    let limited_generation = harness
        .coordinator
        .current(&harness.capability, NOW + 300)
        .expect("second collector generation")
        .generation;
    harness
        .coordinator
        .join_generation(
            &harness.capability,
            limited_generation,
            Duration::from_secs(1),
            NOW + 300,
        )
        .expect("join rate-limited provider result");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    tick(NOW + 1_199, Duration::from_secs(1_199), &mut ticker_state);
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        2,
        "Retry-After remains the maximum dispatch deadline"
    );
    tick(NOW + 1_200, Duration::from_secs(1_200), &mut ticker_state);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 3);
}

#[test]
fn stop_waits_for_collector_admission_and_later_snapshots_exclude_it() {
    use std::thread;

    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::sync_channel(0);
    let executor = Arc::new(GatedExecutor {
        started: started_sender,
        release: Mutex::new(release_receiver),
        calls: AtomicUsize::new(0),
    });
    let executor_trait: Arc<dyn UsageProviderExecutor> = Arc::<GatedExecutor>::clone(&executor);
    let harness = CollectorHarness::new(executor_trait);
    let monitor_id = harness.start_approved_observer();
    let source_id = source_id();
    let store = Arc::clone(&harness.store);
    let publisher = harness.publisher.clone();
    let coordinator = Arc::clone(&harness.coordinator);
    let (snapshot_sender, snapshot_receiver) = mpsc::channel();
    let (resume_sender, resume_receiver) = mpsc::channel();
    let admission_thread = thread::spawn(move || {
        store.with_collection_admission(|selected| {
            snapshot_sender
                .send(selected.to_vec())
                .expect("publish source snapshot to test");
            resume_receiver
                .recv()
                .expect("wait for test to begin collector admission");
            collect_due_for_sources(selected, &publisher, &coordinator, NOW);
            started_receiver
                .recv_timeout(Duration::from_secs(1))
                .expect("the admitted poll starts before the permit is released");
        });
    });
    assert_eq!(
        snapshot_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("read source snapshot"),
        vec![source_id]
    );

    let store = Arc::clone(&harness.store);
    let (stop_result_sender, stop_result_receiver) = mpsc::channel();
    let stop_thread = thread::spawn(move || {
        let result = store.operate(MonitorOperation::Stop { monitor_id }, NOW + 1);
        let _ignored = stop_result_sender.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(1);
    while harness.store.collector_admission_waiters() == 0 {
        assert!(
            Instant::now() < deadline,
            "Stop must reach the admission gate while collector admission is held"
        );
        thread::yield_now();
    }
    assert_eq!(
        harness.store.collector_admission_waiters(),
        1,
        "the Stop call observed the held admission gate and is waiting for it"
    );

    resume_sender
        .send(())
        .expect("allow the protected admission to run");
    admission_thread
        .join()
        .expect("collector admission finishes before Stop");
    assert!(matches!(
        stop_result_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("stop completes after collector admission"),
        Ok(MonitorReply::Stopped { .. })
    ));
    stop_thread.join().expect("join stop operation");
    assert!(harness.store.collection_accounts().is_empty());

    harness.set_coordinator_clock(ClockSample {
        wall_epoch: NOW + 2,
        monotonic_elapsed: Duration::from_secs(2),
    });

    collect_due_for_active_monitors(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        NOW + 2,
    );
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        1,
        "a post-Stop snapshot cannot dispatch another provider call"
    );
    release_sender
        .send(())
        .expect("release admitted provider call");
    let generation = harness
        .coordinator
        .current(&harness.capability, NOW + 2)
        .expect("read admitted generation")
        .generation;
    harness
        .coordinator
        .join_generation(
            &harness.capability,
            generation,
            Duration::from_secs(1),
            NOW + 2,
        )
        .expect("join admitted provider call");
}
