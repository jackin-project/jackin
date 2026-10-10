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
use jackin_protocol::control::{FocusedUsageView, Money};
use jackin_protocol::usage_broker::UsageAccountCapability;
use jackin_protocol::usage_monitor::{
    MonitorAccountBindingInput, MonitorConfig, MonitorOperation, MonitorPolicy,
    MonitorPolicyApprovalInput, MonitorProvider, MonitorPurpose, MonitorReply, MonitorScope,
    SpendRecordInput, SpendRecordSource, StatuslineObservation, StatuslineQuotaWindow,
    StatuslineRateLimits, USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
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

struct TickerHarness {
    store: Arc<MonitorStore>,
    coordinator: Arc<UsageCoordinator>,
    publisher: publish::ProjectionPublisher,
    executor: Arc<CountingExecutor>,
    temp: tempfile::TempDir,
}

impl TickerHarness {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temporary ticker state");
        let executor = Arc::new(CountingExecutor::default());
        let executor_trait: Arc<dyn UsageProviderExecutor> =
            Arc::<CountingExecutor>::clone(&executor);
        let coordinator = Arc::new(UsageCoordinator::new(
            executor_trait,
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
            executor,
            temp,
        }
    }

    fn start_monitor(&self) -> String {
        let binding = match self
            .store
            .operate(
                MonitorOperation::BindAccount {
                    binding: MonitorAccountBindingInput {
                        provider: MonitorProvider::Claude,
                        account_id: ACCOUNT_ID.to_owned(),
                        provider_account_id: None,
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
    assert_eq!(harness.executor.calls.load(Ordering::SeqCst), 0);
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
