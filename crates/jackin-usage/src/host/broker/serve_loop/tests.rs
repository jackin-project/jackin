// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use std::os::unix::net::UnixStream;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::thread;

use crate::coordinator::policy::UsageActivity;
use crate::coordinator::{
    FileAccountStateStore, FileProjectionStateStore, ProviderProbeOutcome, UsageCoordinator,
    UsageCoordinatorConfig, UsageProviderExecutor,
};
use jackin_protocol::control::{
    FocusedUsageView, Money, QuotaBucketView, StatusSlot, UsageConfidence, UsageSeverity,
    UsageSnapshotStatus, UsageSource,
};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationErrorKind, UsageFreshnessPhaseV1,
    UsageProjectionRefreshStateV1, UsageRefreshPhase,
};
use jackin_protocol::usage_monitor::{
    MonitorAccountBindingInput, MonitorConfig, MonitorEvidenceSource, MonitorOperation,
    MonitorPolicy, MonitorPolicyApprovalInput, MonitorProvider, MonitorPurpose, MonitorReply,
    MonitorScope, SpendRecordInput, SpendRecordSource, StatuslineObservation,
    StatuslineQuotaWindow, StatuslineRateLimits, USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
};

const NOW: i64 = 1_800_000_000;
const ACCOUNT_ID: &str = "acct-serve-ticker";

struct ManualTickerClock {
    initial: ClockSample,
    samples: Receiver<ClockSample>,
    ready: mpsc::Sender<()>,
}

impl TickerClock for ManualTickerClock {
    fn initial_sample(&self) -> ClockSample {
        self.initial
    }

    fn next_sample(&mut self) -> Option<ClockSample> {
        self.ready.send(()).ok()?;
        self.samples.recv().ok()
    }
}

#[derive(Default)]
struct CountingExecutor {
    calls: AtomicUsize,
}

impl UsageProviderExecutor for CountingExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ProviderProbeOutcome::success(FocusedUsageView::unavailable("claude", NOW))
    }
}

struct GatedExecutor {
    started: mpsc::Sender<()>,
    release: Mutex<Receiver<()>>,
    calls: AtomicUsize,
}

struct GatedRelease {
    sender: Option<SyncSender<()>>,
}

impl GatedRelease {
    fn release(&mut self) {
        self.sender
            .take()
            .expect("provider release is available")
            .send(())
            .expect("release the deterministic provider result");
    }
}

impl Drop for GatedRelease {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            // Unblock the worker before TickerHarness drops its coordinator,
            // including when an assertion unwinds before the normal release.
            let _send_result = sender.try_send(());
        }
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
            1 => ProviderProbeOutcome::success(successful_provider_view(NOW)),
            2 => ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::RateLimited,
                message: "fixture retry after".to_owned(),
                retry_at_epoch: Some(NOW + 1_200),
            },
            _ => ProviderProbeOutcome::success(successful_provider_view(NOW + 1_200)),
        }
    }
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
        ProviderProbeOutcome::success(successful_provider_view(NOW + 2))
    }
}

fn successful_provider_view(now_epoch: i64) -> FocusedUsageView {
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

struct TickerHarness {
    store: Arc<MonitorStore>,
    coordinator: Arc<UsageCoordinator>,
    publisher: publish::ProjectionPublisher,
    executor: Option<Arc<CountingExecutor>>,
    temp: tempfile::TempDir,
}

impl TickerHarness {
    fn new() -> Self {
        let executor = Arc::new(CountingExecutor::default());
        let executor_trait: Arc<dyn UsageProviderExecutor> =
            Arc::<CountingExecutor>::clone(&executor);
        Self::with_executor(executor_trait, Some(executor))
    }

    fn with_executor(
        executor: Arc<dyn UsageProviderExecutor>,
        counting_executor: Option<Arc<CountingExecutor>>,
    ) -> Self {
        let temp = tempfile::tempdir().expect("temporary ticker state");
        let coordinator = Arc::new(UsageCoordinator::new(
            executor,
            Arc::new(FileAccountStateStore::at(temp.path().join("accounts"))),
            UsageCoordinatorConfig::default(),
        ));
        let publisher = publish::ProjectionPublisher::new(
            Arc::clone(&coordinator),
            Arc::new(Mutex::new(super::super::empty_projection("ticker-test"))),
            FileProjectionStateStore::under_data_dir(temp.path()),
        );
        let store = Arc::new(MonitorStore::open(temp.path()).expect("open monitor state"));
        Self {
            store,
            coordinator,
            publisher,
            executor: counting_executor,
            temp,
        }
    }

    fn start_monitor(&self) -> String {
        self.start_monitor_mapped_to(None)
    }

    fn start_monitor_mapped_to(&self, provider_account_id: Option<String>) -> String {
        let binding = match self
            .store
            .operate(
                MonitorOperation::BindAccount {
                    binding: MonitorAccountBindingInput {
                        provider: MonitorProvider::Claude,
                        account_id: ACCOUNT_ID.to_owned(),
                        provider_account_id,
                        experimental_collector_approved: false,
                        operator_label: "isolated-test-operator".to_owned(),
                        operator_confirmed: true,
                    },
                },
                NOW,
            )
            .expect("bind isolated monitor account")
        {
            MonitorReply::AccountBound { binding } => binding,
            other => panic!("expected account-bound reply, got {other:?}"),
        };
        let observation = StatuslineObservation {
            schema_version: USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
            session_id: "session-ticker".to_owned(),
            model: None,
            claude_code_version: Some("2.1.80".to_owned()),
            rate_limits: StatuslineRateLimits {
                five_hour: Some(StatuslineQuotaWindow {
                    used_percentage_basis_points: Some(2_000),
                    reset_at_epoch: Some(NOW + 3_600),
                }),
                seven_day: Some(StatuslineQuotaWindow {
                    used_percentage_basis_points: Some(1_000),
                    reset_at_epoch: Some(NOW + 7 * 24 * 60 * 60),
                }),
            },
        };
        self.store
            .operate(
                MonitorOperation::Ingest {
                    scope: MonitorScope::BoundAccount {
                        binding_id: binding.binding_id.clone(),
                        binding_revision: binding.revision,
                        session_id: None,
                    },
                    observation,
                },
                NOW,
            )
            .expect("ingest fresh quota evidence");
        self.store
            .operate(
                MonitorOperation::RecordSpend {
                    record: SpendRecordInput {
                        account_id: ACCOUNT_ID.to_owned(),
                        billing_period_start_epoch: NOW - 100,
                        billing_period_end_epoch: NOW + 100_000,
                        amount: Money::new(0, "SGD", 2),
                        evidence_at_epoch: Some(NOW),
                        verified: true,
                        source: SpendRecordSource::OperatorReceipt,
                    },
                },
                NOW,
            )
            .expect("record fresh spend baseline");
        let policy = match self
            .store
            .operate(
                MonitorOperation::ApprovePolicy {
                    approval: MonitorPolicyApprovalInput {
                        binding_id: binding.binding_id.clone(),
                        binding_revision: binding.revision,
                        goal_id: "goal-serve-ticker".to_owned(),
                        new_policy: MonitorPolicy::StrictSgd,
                        budget: Some(Money::new(5_000, "SGD", 2)),
                        operator_label: "isolated-test-operator".to_owned(),
                        operator_confirmed: true,
                        acknowledge_no_sgd_cap: false,
                        expected_revision: None,
                    },
                },
                NOW,
            )
            .expect("approve strict SGD policy in isolated fixture")
        {
            MonitorReply::PolicyApproved { policy } => policy,
            other => panic!("expected policy-approved reply, got {other:?}"),
        };
        let reply = self
            .store
            .operate(
                MonitorOperation::Start {
                    config: MonitorConfig {
                        provider: MonitorProvider::Claude,
                        purpose: MonitorPurpose::DispatchGuard,
                        scope: MonitorScope::BoundAccount {
                            binding_id: binding.binding_id,
                            binding_revision: binding.revision,
                            session_id: None,
                        },
                        goal_id: Some("goal-serve-ticker".to_owned()),
                        expected_model: None,
                        policy_revision: Some(policy.revision),
                        experimental_collector: false,
                    },
                    idempotency_key: "fixture-serve-ticker".to_owned(),
                },
                NOW,
            )
            .expect("start active monitor");
        match reply {
            MonitorReply::Started { status } => {
                assert!(status.runnable);
                status.monitor_id.clone()
            }
            other => panic!("expected started monitor, got {other:?}"),
        }
    }

    fn install_catalog(&mut self, capability: &UsageAccountCapability) {
        self.publisher = publish::ProjectionPublisher::new(
            Arc::clone(&self.coordinator),
            Arc::new(Mutex::new(super::super::empty_projection("ticker-test"))),
            FileProjectionStateStore::under_data_dir(self.temp.path()),
        )
        .with_catalog([UsageCatalogEntry {
            capability: capability.clone(),
            revision: "ticker-catalog-revision".to_owned(),
        }]);
    }

    fn start_approved_collector(&self, provider_account_id: &str) -> String {
        let binding = match self
            .store
            .operate(
                MonitorOperation::BindAccount {
                    binding: MonitorAccountBindingInput {
                        provider: MonitorProvider::Claude,
                        account_id: ACCOUNT_ID.to_owned(),
                        provider_account_id: Some(provider_account_id.to_owned()),
                        experimental_collector_approved: true,
                        operator_label: "isolated-test-operator".to_owned(),
                        operator_confirmed: true,
                    },
                },
                NOW,
            )
            .expect("approve isolated provider-account mapping")
        {
            MonitorReply::AccountBound { binding } => binding,
            other => panic!("expected account-bound reply, got {other:?}"),
        };
        self.store
            .set_experimental_collector_source(Some(provider_account_id.to_owned()));
        let reply = self
            .store
            .operate(
                MonitorOperation::Start {
                    config: MonitorConfig {
                        provider: MonitorProvider::Claude,
                        purpose: MonitorPurpose::ObserveOnly,
                        scope: MonitorScope::BoundAccount {
                            binding_id: binding.binding_id.clone(),
                            binding_revision: binding.revision,
                            session_id: None,
                        },
                        goal_id: None,
                        expected_model: None,
                        policy_revision: None,
                        experimental_collector: true,
                    },
                    idempotency_key: "fixture-approved-ticker-collector".to_owned(),
                },
                NOW,
            )
            .expect("start approved observe-only collector");
        let MonitorReply::Started { status } = reply else {
            panic!("expected started collector, got {reply:?}");
        };
        assert_eq!(
            self.store.collection_accounts(),
            vec![provider_account_id.to_owned()],
            "only the approved mapped observer enters the collector scheduler"
        );
        status.monitor_id
    }

    fn due_account(&self) {
        self.coordinator
            .set_activity(
                &UsageAccountCapability {
                    account_id: "account-with-due-cadence".to_owned(),
                    surface_id: "claude".to_owned(),
                },
                UsageActivity::DirectInteraction,
                false,
                NOW,
            )
            .expect("set cadence activity");
    }

    fn monitor_state_file(&self) -> std::path::PathBuf {
        self.temp
            .path()
            .join(super::super::BROKER_DIR)
            .join("monitor")
            .join("state.json")
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
}

fn spawn_ticker(
    harness: &TickerHarness,
) -> (
    Arc<AtomicBool>,
    SyncSender<ClockSample>,
    Receiver<()>,
    thread::JoinHandle<()>,
) {
    let (sample_sender, sample_receiver) = mpsc::sync_channel(0);
    let (ready_sender, ready_receiver) = mpsc::channel();
    let shutdown = Arc::new(AtomicBool::new(false));
    let clock = ManualTickerClock {
        initial: ClockSample {
            wall_epoch: NOW,
            monotonic_elapsed: Duration::ZERO,
        },
        samples: sample_receiver,
        ready: ready_sender,
    };
    let thread = spawn_publisher_ticker_with_clock(
        harness.publisher.clone(),
        Arc::clone(&harness.coordinator),
        Arc::clone(&harness.store),
        Arc::clone(&shutdown),
        Box::new(clock),
    )
    .expect("spawn ticker thread");
    ready_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("ticker waits for its first fake-clock sample");
    (shutdown, sample_sender, ready_receiver, thread)
}

fn watch_cursor(store: &MonitorStore, monitor_id: &str) -> u64 {
    let reply = store
        .operate(
            MonitorOperation::Watch {
                monitor_id: monitor_id.to_owned(),
                after_sequence: 0,
                timeout_ms: 0,
            },
            NOW,
        )
        .expect("read latest watch event");
    match reply {
        MonitorReply::Watch { events, .. } => {
            events
                .last()
                .expect("started monitor has an event")
                .sequence
        }
        other => panic!("expected monitor watch result, got {other:?}"),
    }
}

fn watch_for_next_event(
    store: Arc<MonitorStore>,
    monitor_id: String,
    cursor: u64,
) -> (Receiver<MonitorReply>, thread::JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel();
    let thread = thread::spawn(move || {
        let reply = store
            .operate(
                MonitorOperation::Watch {
                    monitor_id,
                    after_sequence: cursor,
                    timeout_ms: 5_000,
                },
                NOW,
            )
            .expect("watch next monitor event");
        let _ignored = sender.send(reply);
    });
    (receiver, thread)
}

fn wait_ticker_ready(ready: &Receiver<()>) {
    ready
        .recv_timeout(Duration::from_secs(1))
        .expect("ticker completed its previous sample");
}

fn assert_watch_event(receiver: &Receiver<MonitorReply>) {
    match receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("ticker change wakes monitor watch")
    {
        MonitorReply::Watch {
            events,
            timed_out: false,
            ..
        } => assert!(!events.is_empty()),
        other => panic!("expected a monitor event, got {other:?}"),
    }
}

fn shutdown_ticker(
    shutdown: Arc<AtomicBool>,
    samples: SyncSender<ClockSample>,
    ticker: thread::JoinHandle<()>,
) {
    shutdown.store(true, Ordering::Release);
    drop(samples);
    ticker.join().expect("join fake-clock ticker");
}

fn connection_context(
    harness: &TickerHarness,
    shutdown: Arc<AtomicBool>,
    fenced: Arc<AtomicBool>,
) -> Arc<ConnectionContext> {
    let build_id: Arc<str> = Arc::from("serve-test");
    let wait_pool = Arc::new(waits::WaitPool::new(
        Arc::clone(&harness.coordinator),
        Arc::clone(&build_id),
        harness.publisher.clone(),
        Arc::clone(&harness.store),
        Arc::clone(&shutdown),
        None,
    ));
    Arc::new(ConnectionContext {
        coordinator: Arc::clone(&harness.coordinator),
        build_id,
        publisher: harness.publisher.clone(),
        monitor_store: Arc::clone(&harness.store),
        shutdown,
        fenced,
        catalog_refresh: None,
        wait_pool,
        collector_liveness: None,
    })
}

#[test]
fn spawned_ticker_reconciles_one_sleep_jump_and_wakes_monitor_watch() {
    let harness = TickerHarness::new();
    harness.due_account();
    let monitor_id = harness.start_monitor();
    assert!(harness.store.has_active());
    let cursor = watch_cursor(&harness.store, &monitor_id);
    let (watch_result, watcher) =
        watch_for_next_event(Arc::clone(&harness.store), monitor_id, cursor);
    let (shutdown, samples, ready, ticker) = spawn_ticker(&harness);
    let sleep_jump = ClockSample {
        wall_epoch: NOW + 11 * 60,
        monotonic_elapsed: Duration::from_millis(200),
    };
    assert!(wall_clock_wake_detected(
        NOW,
        sleep_jump.monotonic_elapsed,
        sleep_jump.wall_epoch,
    ));
    samples.send(sleep_jump).expect("send post-sleep sample");
    wait_ticker_ready(&ready);

    assert_watch_event(&watch_result);
    assert_eq!(
        harness
            .executor
            .as_ref()
            .expect("counting executor in ticker harness")
            .calls
            .load(Ordering::SeqCst),
        0
    );
    let after_idle_window = ClockSample {
        wall_epoch: NOW + 22 * 60,
        monotonic_elapsed: Duration::from_mins(11),
    };
    samples
        .send(after_idle_window)
        .expect("send sample after eleven idle minutes");
    wait_ticker_ready(&ready);

    assert!(
        !should_exit_idle(
            after_idle_window.monotonic_elapsed,
            Duration::ZERO,
            Duration::from_mins(10),
            harness.coordinator.is_idle(),
            harness.store.has_active(),
            false,
        ),
        "an active monitor keeps the service out of idle exit after wake"
    );
    assert!(should_exit_idle(
        after_idle_window.monotonic_elapsed,
        Duration::ZERO,
        Duration::from_mins(10),
        true,
        false,
        false,
    ));
    assert!(
        !should_exit_idle(
            after_idle_window.monotonic_elapsed,
            Duration::ZERO,
            Duration::from_mins(10),
            true,
            false,
            true,
        ),
        "foreground collector liveness keeps a prepared broker alive beyond the idle timeout"
    );

    shutdown_ticker(shutdown, samples, ticker);
    watcher.join().expect("join watch thread");
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "One fake-clock integration regression covers approved collection, terminal publication, observation retry, watch notification, and both quota windows."
)]
fn ticker_publishes_completed_provider_evidence_for_approved_observer() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::sync_channel(1);
    let executor = Arc::new(GatedExecutor {
        started: started_sender,
        release: Mutex::new(release_receiver),
        calls: AtomicUsize::new(0),
    });
    let executor_trait: Arc<dyn UsageProviderExecutor> = Arc::<GatedExecutor>::clone(&executor);
    let mut harness = TickerHarness::with_executor(executor_trait, None);
    // Declared after the harness so panic cleanup releases the provider before
    // coordinator teardown joins its worker thread.
    let mut release = GatedRelease {
        sender: Some(release_sender),
    };
    let capability = UsageAccountCapability {
        account_id: "provider-canonical-serve-ticker".to_owned(),
        surface_id: "claude".to_owned(),
    };
    harness.install_catalog(&capability);
    let monitor_id = harness.start_approved_collector(&capability.account_id);
    assert!(harness.store.has_active());
    let mut ticker_state = harness.ticker_state();

    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW,
            monotonic_elapsed: Duration::from_millis(200),
        },
        &mut ticker_state,
    );
    started_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("approved observer starts a provider generation through the ticker");
    let generation = harness
        .coordinator
        .current(&capability, NOW)
        .expect("load the active provider generation")
        .generation;
    let updating_projection = harness
        .publisher
        .current_projection()
        .expect("read updating projection");
    assert_eq!(
        updating_projection.refresh_state,
        UsageProjectionRefreshStateV1::Refreshing
    );
    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 1,
            monotonic_elapsed: Duration::from_millis(1_200),
        },
        &mut ticker_state,
    );
    let observed_updating_projection = harness
        .publisher
        .current_projection()
        .expect("read updating projection after the ticker step");
    assert_eq!(
        observed_updating_projection.refresh_state,
        UsageProjectionRefreshStateV1::Refreshing
    );
    assert_eq!(
        ticker_state.last_observed_projection_id.as_deref(),
        Some(observed_updating_projection.projection_id.as_str()),
        "the ticker observes the in-flight projection before terminal publication"
    );
    let cursor = watch_cursor(&harness.store, &monitor_id);
    let (watch_result, watcher) =
        watch_for_next_event(Arc::clone(&harness.store), monitor_id.clone(), cursor);
    let state_file = harness.monitor_state_file();
    let backup_file = state_file.with_extension("state-backup");
    fs::rename(&state_file, &backup_file).expect("move durable state file for test fault");
    symlink(&backup_file, &state_file).expect("install test-only state symlink");

    release.release();
    let completed = harness
        .coordinator
        .join_generation(&capability, generation, Duration::from_secs(1), NOW + 2)
        .expect("wait for provider completion without a client refresh");
    assert_eq!(completed.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert!(harness.coordinator.is_idle());
    assert!(harness.store.has_active());

    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 2,
            monotonic_elapsed: Duration::from_millis(2_200),
        },
        &mut ticker_state,
    );
    let completed_projection = harness
        .publisher
        .current_projection()
        .expect("read completed projection");
    assert_eq!(
        completed_projection.refresh_state,
        UsageProjectionRefreshStateV1::Idle
    );
    assert!(harness.coordinator.is_idle());
    assert!(
        harness.store.has_active(),
        "the approved observe-only monitor remains active after terminal publication"
    );
    let projected_account = completed_projection
        .providers
        .iter()
        .flat_map(|provider| &provider.accounts)
        .find(|account| account.canonical_account_id == capability.account_id)
        .expect("completed account is published");
    assert_eq!(
        projected_account.freshness.phase,
        UsageFreshnessPhaseV1::Current
    );

    assert_eq!(
        ticker_state.last_observed_projection_id.as_deref(),
        Some(observed_updating_projection.projection_id.as_str()),
        "failed monitor persistence leaves the final projection pending"
    );
    let observation_retry_after = ticker_state.observation_retry_after;
    let last_publish_attempt = ticker_state.last_publish_attempt;
    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 3,
            monotonic_elapsed: Duration::from_millis(2_400),
        },
        &mut ticker_state,
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        ticker_state.observation_retry_after,
        observation_retry_after
    );
    assert_eq!(ticker_state.last_publish_attempt, last_publish_attempt);

    fs::remove_file(&state_file).expect("remove test-only symlink");
    fs::rename(&backup_file, &state_file).expect("restore durable monitor state file");
    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 3,
            monotonic_elapsed: Duration::from_millis(3_200),
        },
        &mut ticker_state,
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        ticker_state.last_observed_projection_id.as_deref(),
        Some(completed_projection.projection_id.as_str())
    );
    assert_watch_event(&watch_result);
    watcher.join().expect("join final projection watch");
    let MonitorReply::Status { status } = harness
        .store
        .operate(MonitorOperation::Status { monitor_id }, NOW + 3)
        .expect("read monitor status after ticker publication")
    else {
        panic!("expected monitor status after provider completion");
    };
    assert_eq!(status.five_hour.used_percentage_basis_points, Some(3_600));
    assert_eq!(status.five_hour.reset_at_epoch, Some(NOW + 2 + 3_600));
    assert_eq!(status.seven_day.used_percentage_basis_points, Some(2_800));
    assert_eq!(
        status.seven_day.reset_at_epoch,
        Some(NOW + 2 + 7 * 24 * 60 * 60)
    );
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
                .expect("five-hour provider evidence")
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
                .expect("seven-day provider evidence")
                .evidence_sequence
        ),
        Some(MonitorEvidenceSource::BrokerProjection)
    );

    let completed_projection_id = completed_projection.projection_id;
    let last_publish_attempt = ticker_state.last_publish_attempt;
    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 3,
            monotonic_elapsed: Duration::from_millis(2_400),
        },
        &mut ticker_state,
    );
    assert_eq!(ticker_state.last_publish_attempt, last_publish_attempt);
    assert_eq!(
        harness
            .publisher
            .current_projection()
            .expect("read unchanged projection")
            .projection_id,
        completed_projection_id,
        "unchanged idle ticks do not publish a new projection"
    );
}

#[test]
fn ticker_collector_preserves_minimum_attempt_floor_and_retry_after() {
    let executor = Arc::new(RetryAfterExecutor {
        calls: AtomicUsize::new(0),
    });
    let executor_trait: Arc<dyn UsageProviderExecutor> =
        Arc::<RetryAfterExecutor>::clone(&executor);
    let mut harness = TickerHarness::with_executor(executor_trait, None);
    let capability = UsageAccountCapability {
        account_id: ACCOUNT_ID.to_owned(),
        surface_id: "claude".to_owned(),
    };
    harness.install_catalog(&capability);
    harness.start_approved_collector(&capability.account_id);
    let mut ticker_state = harness.ticker_state();

    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW,
            monotonic_elapsed: Duration::from_millis(200),
        },
        &mut ticker_state,
    );
    let first_generation = harness
        .coordinator
        .current(&capability, NOW)
        .expect("first collector generation")
        .generation;
    harness
        .coordinator
        .join_generation(&capability, first_generation, Duration::from_secs(1), NOW)
        .expect("join first successful provider result");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    let last_collection_check = ticker_state.last_collection_check;
    let last_publish_attempt = ticker_state.last_publish_attempt;
    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 1,
            monotonic_elapsed: Duration::from_millis(400),
        },
        &mut ticker_state,
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(ticker_state.last_collection_check, last_collection_check);
    assert_eq!(ticker_state.last_publish_attempt, last_publish_attempt);

    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 299,
            monotonic_elapsed: Duration::from_secs(299),
        },
        &mut ticker_state,
    );
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        1,
        "a fresh provider result is not polled before Claude's 300-second floor"
    );

    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 300,
            monotonic_elapsed: Duration::from_mins(5),
        },
        &mut ticker_state,
    );
    let limited_generation = harness
        .coordinator
        .current(&capability, NOW + 300)
        .expect("second collector generation")
        .generation;
    harness
        .coordinator
        .join_generation(
            &capability,
            limited_generation,
            Duration::from_secs(1),
            NOW + 300,
        )
        .expect("join provider retry-after result");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 1_199,
            monotonic_elapsed: Duration::from_secs(1_199),
        },
        &mut ticker_state,
    );
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        2,
        "the retry-after deadline remains the maximum refresh floor"
    );

    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 1_200,
            monotonic_elapsed: Duration::from_mins(20),
        },
        &mut ticker_state,
    );
    let final_generation = harness
        .coordinator
        .current(&capability, NOW + 1_200)
        .expect("final collector generation")
        .generation;
    harness
        .coordinator
        .join_generation(
            &capability,
            final_generation,
            Duration::from_secs(1),
            NOW + 1_200,
        )
        .expect("join provider generation after Retry-After");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 3);
}

#[test]
fn spawned_ticker_retries_a_failed_due_tick_after_one_second() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let harness = TickerHarness::new();
    let monitor_id = harness.start_monitor();
    let cursor = watch_cursor(&harness.store, &monitor_id);
    let (watch_result, watcher) =
        watch_for_next_event(Arc::clone(&harness.store), monitor_id, cursor);
    let state_file = harness.monitor_state_file();
    let backup_file = state_file.with_extension("state-backup");
    fs::rename(&state_file, &backup_file).expect("move durable state file for test fault");
    symlink(&backup_file, &state_file).expect("install test-only state symlink");

    let (shutdown, samples, ready, ticker) = spawn_ticker(&harness);
    samples
        .send(ClockSample {
            wall_epoch: NOW + 11 * 60,
            monotonic_elapsed: Duration::from_millis(200),
        })
        .expect("send due sample with persistence failure");
    wait_ticker_ready(&ready);
    assert!(matches!(watch_result.try_recv(), Err(TryRecvError::Empty)));

    fs::remove_file(&state_file).expect("remove test-only state symlink");
    fs::rename(&backup_file, &state_file).expect("restore durable monitor state");
    samples
        .send(ClockSample {
            wall_epoch: NOW + 11 * 60,
            monotonic_elapsed: Duration::from_millis(400),
        })
        .expect("send sample before retry deadline");
    wait_ticker_ready(&ready);
    assert!(
        matches!(watch_result.try_recv(), Err(TryRecvError::Empty)),
        "a failed tick is retried after a bounded delay, not every ticker interval"
    );
    samples
        .send(ClockSample {
            wall_epoch: NOW + 11 * 60,
            monotonic_elapsed: Duration::from_millis(800),
        })
        .expect("send another sample before retry deadline");
    wait_ticker_ready(&ready);
    assert!(matches!(watch_result.try_recv(), Err(TryRecvError::Empty)));
    samples
        .send(ClockSample {
            wall_epoch: NOW + 11 * 60,
            monotonic_elapsed: Duration::from_millis(1_200),
        })
        .expect("send sample at retry deadline");
    wait_ticker_ready(&ready);
    assert_watch_event(&watch_result);

    shutdown_ticker(shutdown, samples, ticker);
    watcher.join().expect("join watch thread");
}

#[test]
fn lease_renewal_due_uses_wall_or_monotonic_elapsed_time() {
    let last = ClockSample {
        wall_epoch: NOW,
        monotonic_elapsed: Duration::ZERO,
    };
    assert!(interval_elapsed(
        ClockSample {
            wall_epoch: NOW + 10,
            monotonic_elapsed: Duration::from_millis(100),
        },
        last,
        Duration::from_secs(10),
    ));
    assert!(interval_elapsed(
        ClockSample {
            wall_epoch: NOW,
            monotonic_elapsed: Duration::from_secs(10),
        },
        last,
        Duration::from_secs(10),
    ));
    assert!(!interval_elapsed(
        ClockSample {
            wall_epoch: NOW + 9,
            monotonic_elapsed: Duration::from_secs(9),
        },
        last,
        Duration::from_secs(10),
    ));
}

#[test]
fn fenced_connection_workers_drop_queued_streams_before_dispatch() {
    use std::io::Read as _;

    let harness = TickerHarness::new();
    let shutdown = Arc::new(AtomicBool::new(false));
    let fenced = Arc::new(AtomicBool::new(true));
    let context = connection_context(&harness, shutdown, fenced);
    let (connections, receiver) = mpsc::sync_channel(1);
    let workers = spawn_connection_workers(Arc::new(Mutex::new(receiver)), context);
    let (mut client, server) = UnixStream::pair().expect("create local broker stream");
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set bounded stream read");
    connections
        .send(server)
        .expect("queue stream after the owner fence");
    drop(connections);

    let mut response = [0_u8; 1];
    assert_eq!(
        client.read(&mut response).expect("read closed stream"),
        0,
        "a fenced worker discards queued requests without dispatching them"
    );
    for worker in workers {
        worker.join().expect("join fenced worker");
    }
    assert!(!harness.store.has_active());
}
