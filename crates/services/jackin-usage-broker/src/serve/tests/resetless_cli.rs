// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::collector::{CollectorHarness, provider_view};
use super::*;

use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command as ProcessCommand, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jackin_protocol::control::{CanonicalRemainingPercent, FocusedUsageView, Money};
use jackin_protocol::usage_broker::UsageAccountCapability;
use jackin_protocol::usage_monitor::{
    MonitorAccountBinding, MonitorAccountBindingInput, MonitorConfig, MonitorIssueCode,
    MonitorOperation, MonitorPolicy, MonitorPolicyApprovalInput, MonitorProvider, MonitorPurpose,
    MonitorReply, MonitorScope, SpendRecordInput, SpendRecordSource, StatuslineObservation,
    StatuslineRateLimits, USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
};
use jackin_usage_coordinator::{ProviderProbeOutcome, UsageProviderExecutor};

const RESETLESS_SESSION: &str = "resetless-cli-session";
const GOAL_ID: &str = "goal-resetless-cli";
const SESSION_RESET_EPOCH: i64 = NOW + 301;
const LONG_RESET_EPOCH: i64 = NOW + 24 * 60 * 60;
const PROBE_START_TIMEOUT: Duration = Duration::from_secs(3);
const PROBE_GATE_TIMEOUT: Duration = Duration::from_secs(10);
const LONG_WATCH_TIMEOUT_SECS: u64 = 90;
const LONG_WATCH_FINISH_TIMEOUT_SECS: u64 = LONG_WATCH_TIMEOUT_SECS + 5;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProbeCall {
    account_id: String,
    surface_id: String,
    generation: u64,
}

struct ScriptedExecutor {
    calls: AtomicUsize,
    requested: Mutex<Vec<ProbeCall>>,
    started: Sender<ProbeCall>,
    releases: Mutex<VecDeque<Receiver<()>>>,
    results: Mutex<VecDeque<FocusedUsageView>>,
}

impl ScriptedExecutor {
    fn new(
        results: impl IntoIterator<Item = FocusedUsageView>,
    ) -> (Self, Receiver<ProbeCall>, Vec<Sender<()>>) {
        let (started, start_receiver) = mpsc::channel();
        let mut result_queue = VecDeque::new();
        let mut release_queue = VecDeque::new();
        let mut release_senders = Vec::new();
        for result in results {
            let (release_sender, release_receiver) = mpsc::channel();
            result_queue.push_back(result);
            release_queue.push_back(release_receiver);
            release_senders.push(release_sender);
        }
        (
            Self {
                calls: AtomicUsize::new(0),
                requested: Mutex::new(Vec::new()),
                started,
                releases: Mutex::new(release_queue),
                results: Mutex::new(result_queue),
            },
            start_receiver,
            release_senders,
        )
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::Acquire)
    }

    fn requested(&self) -> Vec<ProbeCall> {
        self.requested.lock().expect("executor call log").clone()
    }
}

impl UsageProviderExecutor for ScriptedExecutor {
    fn probe(&self, capability: &UsageAccountCapability, generation: u64) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::AcqRel);
        let call = ProbeCall {
            account_id: capability.account_id.clone(),
            surface_id: capability.surface_id.clone(),
            generation,
        };
        self.requested
            .lock()
            .expect("executor call log")
            .push(call.clone());
        self.started
            .send(call)
            .expect("test observes every scheduled provider invocation");
        self.releases
            .lock()
            .expect("scripted provider release gates")
            .pop_front()
            .expect("provider was called more times than scheduled")
            .recv_timeout(PROBE_GATE_TIMEOUT)
            .expect("test releases the scheduled provider generation before its deadline");
        let view = self
            .results
            .lock()
            .expect("scripted provider results")
            .pop_front()
            .expect("provider was called more times than scheduled");
        ProviderProbeOutcome::success(view)
    }
}

fn provider_view_with_windows(
    fetched_at_epoch: i64,
    session_used_basis_points: i32,
    session_reset: Option<i64>,
    long_used_basis_points: i32,
    long_reset: i64,
) -> FocusedUsageView {
    let mut view = provider_view(fetched_at_epoch);
    for (index, used_basis_points, reset) in [
        (0, session_used_basis_points, session_reset),
        (1, long_used_basis_points, Some(long_reset)),
    ] {
        assert!((0..=10_000).contains(&used_basis_points));
        let bucket = &mut view.buckets[index];
        bucket.remaining_percent = Some(
            CanonicalRemainingPercent::new(100.0 - f64::from(used_basis_points) / 100.0)
                .expect("scripted remaining quota percentage is valid"),
        );
        bucket.resets_at = reset;
    }
    view
}

fn bind_collector_source(harness: &CollectorHarness) -> MonitorAccountBinding {
    let source = source_id();
    match harness
        .store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: "collector-test-account".to_owned(),
                    operator_label: "resetless CLI fixture".to_owned(),
                    operator_confirmed: true,
                    provider_account_id: Some(source),
                    experimental_collector_approved: true,
                },
            },
            NOW,
        )
        .expect("bind the injected foreground collector source")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    }
}

fn seed_guard_model_and_spend(harness: &CollectorHarness, binding: &MonitorAccountBinding) {
    harness
        .store
        .operate(
            MonitorOperation::Ingest {
                scope: MonitorScope::BoundAccount {
                    binding_id: binding.binding_id.clone(),
                    binding_revision: binding.revision,
                    session_id: None,
                },
                observation: model_observation(RESETLESS_SESSION),
            },
            NOW,
        )
        .expect("record a fresh expected model for the goal monitor");
    harness
        .store
        .operate(
            MonitorOperation::RecordSpend {
                record: SpendRecordInput {
                    account_id: binding.account_id.clone(),
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
        .expect("record a verified spend baseline for the goal monitor");
}

fn approve_guard_policy(
    harness: &CollectorHarness,
    binding: &MonitorAccountBinding,
) -> jackin_protocol::usage_monitor::MonitorPolicyRecord {
    match harness
        .store
        .operate(
            MonitorOperation::ApprovePolicy {
                approval: MonitorPolicyApprovalInput {
                    binding_id: binding.binding_id.clone(),
                    binding_revision: binding.revision,
                    goal_id: GOAL_ID.to_owned(),
                    new_policy: MonitorPolicy::StrictSgd,
                    budget: Some(Money::new(5_000, "SGD", 2)),
                    operator_label: "resetless CLI fixture".to_owned(),
                    operator_confirmed: true,
                    acknowledge_no_sgd_cap: false,
                    expected_revision: None,
                },
            },
            NOW,
        )
        .expect("approve the isolated strict spend policy")
    {
        MonitorReply::PolicyApproved { policy } => policy,
        other => panic!("expected policy-approved reply, got {other:?}"),
    }
}

fn assert_experimental_guard_start_is_atomic(
    harness: &CollectorHarness,
    binding: &MonitorAccountBinding,
    policy: &jackin_protocol::usage_monitor::MonitorPolicyRecord,
) {
    let state_before_rejected_start = monitor_state(harness);
    let rejected_issue = harness
        .store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    purpose: MonitorPurpose::DispatchGuard,
                    scope: MonitorScope::BoundAccount {
                        binding_id: binding.binding_id.clone(),
                        binding_revision: binding.revision,
                        session_id: None,
                    },
                    goal_id: Some(GOAL_ID.to_owned()),
                    expected_model: Some("claude-sonnet-4-5".to_owned()),
                    policy_revision: Some(policy.revision),
                    experimental_collector: true,
                },
                idempotency_key: "resetless-cli-rejected-experimental-dispatch".to_owned(),
            },
            NOW,
        )
        .expect_err("experimental dispatch must fail before creating a monitor");
    assert_eq!(rejected_issue.code, MonitorIssueCode::ObservationOnly);
    assert_eq!(
        rejected_issue.message,
        "experimental collection is available only to observation-only monitors"
    );
    assert_eq!(rejected_issue.retry_at_epoch, None);
    assert_eq!(monitor_state(harness), state_before_rejected_start);
    assert!(
        harness.store.collection_accounts().is_empty(),
        "a rejected experimental dispatch guard cannot admit collector polling"
    );
}

fn start_authorized_guard(
    harness: &CollectorHarness,
    binding: &MonitorAccountBinding,
    policy: &jackin_protocol::usage_monitor::MonitorPolicyRecord,
) -> String {
    let status = match harness
        .store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    purpose: MonitorPurpose::DispatchGuard,
                    scope: MonitorScope::BoundAccount {
                        binding_id: binding.binding_id.clone(),
                        binding_revision: binding.revision,
                        session_id: None,
                    },
                    goal_id: Some(GOAL_ID.to_owned()),
                    expected_model: Some("claude-sonnet-4-5".to_owned()),
                    policy_revision: Some(policy.revision),
                    experimental_collector: false,
                },
                idempotency_key: "resetless-cli-dispatch-guard".to_owned(),
            },
            NOW,
        )
        .expect("start the active dispatch guard")
    {
        MonitorReply::Started { status } => status,
        other => panic!("expected started reply, got {other:?}"),
    };
    assert_eq!(
        status.account_id.as_deref(),
        Some(binding.account_id.as_str())
    );
    assert_eq!(status.purpose, MonitorPurpose::DispatchGuard);
    assert_eq!(status.goal_id.as_deref(), Some(GOAL_ID));
    status.monitor_id.clone()
}

fn start_dispatch_monitor(harness: &CollectorHarness) -> (MonitorAccountBinding, String) {
    let binding = bind_collector_source(harness);
    seed_guard_model_and_spend(harness, &binding);
    let policy = approve_guard_policy(harness, &binding);
    assert_experimental_guard_start_is_atomic(harness, &binding, &policy);
    let monitor_id = start_authorized_guard(harness, &binding, &policy);
    (binding, monitor_id)
}

fn start_observer_monitor(harness: &CollectorHarness, binding: &MonitorAccountBinding) -> String {
    let source_id = binding
        .provider_account_id
        .clone()
        .expect("confirmed collector binding retains the exact source handle");
    assert!(
        harness.store.collection_accounts().is_empty(),
        "a noncollector dispatch guard cannot enter collector polling"
    );
    let reply = harness
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
                idempotency_key: "resetless-cli-collector-observer".to_owned(),
            },
            NOW,
        )
        .expect("start the separately opted-in collection observer");
    let MonitorReply::Started { status } = reply else {
        panic!("expected the collection observer to start, got {reply:?}");
    };
    assert_eq!(status.purpose, MonitorPurpose::ObserveOnly);
    assert_eq!(
        status.account_id.as_deref(),
        Some(binding.account_id.as_str())
    );
    assert_eq!(status.goal_id, None);
    assert_eq!(harness.store.collection_accounts(), vec![source_id]);
    status.monitor_id.clone()
}

fn model_observation(session_id: &str) -> StatuslineObservation {
    StatuslineObservation {
        schema_version: USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
        session_id: session_id.to_owned(),
        model: Some("claude-sonnet-4-5".to_owned()),
        claude_code_version: Some("2.1.80".to_owned()),
        rate_limits: StatuslineRateLimits {
            five_hour: None,
            seven_day: None,
        },
    }
}

fn refresh_model_for_recovery(harness: &CollectorHarness, binding: &MonitorAccountBinding) {
    harness
        .store
        .operate(
            MonitorOperation::Ingest {
                scope: MonitorScope::BoundAccount {
                    binding_id: binding.binding_id.clone(),
                    binding_revision: binding.revision,
                    session_id: None,
                },
                observation: model_observation("resetless-cli-recovery-model"),
            },
            NOW + 900,
        )
        .expect("record a fresh matching model for post-grace recovery");
}

struct FakeTicker {
    shutdown: Arc<AtomicBool>,
    samples: Option<SyncSender<ClockSample>>,
    ready: Receiver<()>,
    worker: Option<JoinHandle<()>>,
}

impl FakeTicker {
    fn start(harness: &CollectorHarness) -> Self {
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
        let worker = spawn_publisher_ticker_with_clock(
            harness.publisher.clone(),
            Arc::clone(&harness.coordinator),
            Arc::clone(&harness.store),
            Arc::clone(&shutdown),
            Box::new(clock),
        )
        .expect("spawn the production publisher ticker with a controlled clock");
        ready_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("publisher ticker waits for its initial fake-clock sample");
        Self {
            shutdown,
            samples: Some(sample_sender),
            ready: ready_receiver,
            worker: Some(worker),
        }
    }

    fn tick(&self, harness: &CollectorHarness, wall_epoch: i64) {
        let elapsed = u64::try_from(wall_epoch.saturating_sub(NOW))
            .expect("test fake clock never moves before its initial time");
        let sample = ClockSample {
            wall_epoch,
            monotonic_elapsed: Duration::from_secs(elapsed),
        };
        harness.set_coordinator_clock(sample);
        self.samples
            .as_ref()
            .expect("ticker sample channel remains open")
            .send(sample)
            .expect("send a fake-clock sample to the production ticker");
        self.ready
            .recv_timeout(Duration::from_secs(2))
            .expect("publisher ticker completed the fake-clock sample");
    }
}

impl Drop for FakeTicker {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.samples.take();
        if let Some(worker) = self.worker.take() {
            worker.join().expect("join production publisher ticker");
        }
    }
}

struct LocalBroker {
    data_dir: PathBuf,
    socket_path: PathBuf,
    accepting: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    fenced: Arc<AtomicBool>,
    accepted: Arc<AtomicUsize>,
    connections: Option<SyncSender<UnixStream>>,
    acceptor: Option<JoinHandle<()>>,
    workers: Vec<JoinHandle<()>>,
    cli_root: tempfile::TempDir,
}

fn accept_broker_connections(
    listener: &UnixListener,
    accepting: &AtomicBool,
    accepted_count: &AtomicUsize,
    connection_sender: &SyncSender<UnixStream>,
) {
    while accepting.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                accepted_count.fetch_add(1, Ordering::AcqRel);
                if connection_sender.send(stream).is_err() {
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::park_timeout(Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }
}

impl LocalBroker {
    fn start(harness: &CollectorHarness) -> Self {
        let data_dir = harness.temp.path();
        let broker_dir = data_dir.join(crate::BROKER_DIR);
        let run_dir = broker_dir.join(crate::BROKER_RUN_DIR);
        fs::create_dir_all(&run_dir).expect("create isolated broker run directory");
        fs::set_permissions(&broker_dir, fs::Permissions::from_mode(0o700))
            .expect("restrict isolated broker directory");
        fs::set_permissions(&run_dir, fs::Permissions::from_mode(0o700))
            .expect("restrict isolated broker run directory");

        let config = crate::UsageBrokerConfig::for_data_dir(data_dir.to_path_buf());
        let socket_path = config
            .prepare_socket_path()
            .expect("prepare the production broker socket path");
        let listener = bound_socket(&socket_path);
        listener
            .set_nonblocking(true)
            .expect("make the isolated broker accept loop stoppable");

        let (connections, receiver) = mpsc::sync_channel(BROKER_CONNECTION_QUEUE);
        let shutdown = Arc::new(AtomicBool::new(false));
        let fenced = Arc::new(AtomicBool::new(false));
        let build_id: Arc<str> = Arc::from(config.build_id.as_str());
        let wait_pool = Arc::new(waits::WaitPool::new(
            Arc::clone(&harness.coordinator),
            Arc::clone(&build_id),
            harness.publisher.clone(),
            Arc::clone(&harness.store),
            Arc::clone(&shutdown),
            None,
        ));
        let context = Arc::new(ConnectionContext {
            coordinator: Arc::clone(&harness.coordinator),
            build_id,
            publisher: harness.publisher.clone(),
            monitor_store: Arc::clone(&harness.store),
            shutdown: Arc::clone(&shutdown),
            fenced: Arc::clone(&fenced),
            catalog_refresh: None,
            wait_pool,
        });
        let workers = spawn_connection_workers(Arc::new(Mutex::new(receiver)), context);
        assert_eq!(workers.len(), BROKER_CONNECTION_WORKERS);

        let accepting = Arc::new(AtomicBool::new(true));
        let accepted = Arc::new(AtomicUsize::new(0));
        let accept_loop = Arc::clone(&accepting);
        let accepted_count = Arc::clone(&accepted);
        let connection_sender = connections.clone();
        let acceptor = thread::Builder::new()
            .name("resetless-cli-broker-accept".to_owned())
            .spawn(move || {
                accept_broker_connections(
                    &listener,
                    &accept_loop,
                    &accepted_count,
                    &connection_sender,
                );
                drop(listener);
            })
            .expect("spawn isolated production broker accept loop");

        let cli_root = tempfile::tempdir().expect("create isolated CLI home");
        for name in ["home", "jackin-home", "config"] {
            let path = cli_root.path().join(name);
            fs::create_dir(&path).expect("create isolated CLI directory");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                .expect("restrict isolated CLI directory");
        }
        Self {
            data_dir: data_dir.to_path_buf(),
            socket_path,
            accepting,
            shutdown,
            fenced,
            accepted,
            connections: Some(connections),
            acceptor: Some(acceptor),
            workers,
            cli_root,
        }
    }

    fn command(&self, binary: &Path) -> ProcessCommand {
        let mut command = ProcessCommand::new(binary);
        command
            .env_clear()
            .env("HOME", self.cli_root.path().join("home"))
            .env("JACKIN_HOME_DIR", self.cli_root.path().join("jackin-home"))
            .env("JACKIN_CONFIG_DIR", self.cli_root.path().join("config"))
            .env("TMPDIR", std::env::temp_dir())
            .current_dir(self.cli_root.path().join("home"))
            .arg("usage")
            .arg("--data-dir")
            .arg(self.data_dir())
            .arg("--format")
            .arg("json");
        command
    }

    fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn accepted_count(&self) -> usize {
        self.accepted.load(Ordering::Acquire)
    }

    fn wait_for_accepts(&self, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.accepted_count() < expected {
            assert!(
                Instant::now() < deadline,
                "CLI did not use the isolated broker socket"
            );
            thread::park_timeout(Duration::from_millis(5));
        }
    }
}

impl Drop for LocalBroker {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.fenced.store(true, Ordering::Release);
        self.accepting.store(false, Ordering::Release);
        if let Some(acceptor) = self.acceptor.take() {
            acceptor.join().expect("join isolated broker accept loop");
        }
        self.connections.take();
        for worker in self.workers.drain(..) {
            worker
                .join()
                .expect("join isolated production connection worker");
        }
        let _ignored = fs::remove_file(&self.socket_path);
    }
}

#[derive(Debug, Clone)]
struct DurableWatchBoundary {
    state: serde_json::Value,
    target_cursor: u64,
    events: Vec<serde_json::Value>,
}

impl DurableWatchBoundary {
    fn from_state(state: serde_json::Value, monitor_id: &str, after_sequence: u64) -> Self {
        let monitor = &state["monitors"][monitor_id];
        let target_cursor = monitor["next_event_sequence"]
            .as_u64()
            .expect("durable monitor state includes its next event sequence");
        assert!(
            target_cursor >= after_sequence,
            "durable watch cursor regressed for {monitor_id}: after={after_sequence}, target={target_cursor}, events={}",
            monitor["events"]
        );
        let retained = monitor["events"]
            .as_array()
            .expect("durable monitor state retains its watch events");
        assert_eq!(
            retained.last().and_then(|event| event["sequence"].as_u64()),
            Some(target_cursor),
            "durable target is not represented by the last retained event for {monitor_id}: target={target_cursor}, events={retained:?}"
        );
        assert!(
            retained.iter().all(|event| event["sequence"]
                .as_u64()
                .is_some_and(|sequence| sequence <= target_cursor)),
            "durable watch event exceeds its target for {monitor_id}: target={target_cursor}, events={retained:?}"
        );
        let events = retained
            .iter()
            .filter(|event| {
                event["sequence"]
                    .as_u64()
                    .is_some_and(|sequence| sequence > after_sequence)
            })
            .cloned()
            .collect::<Vec<_>>();
        let expected_count = target_cursor - after_sequence;
        assert_eq!(
            u64::try_from(events.len()).expect("retained event count fits u64"),
            expected_count,
            "durable watch suffix is incomplete for {monitor_id}: after={after_sequence}, target={target_cursor}, events={events:?}"
        );
        for (index, event) in events.iter().enumerate() {
            let offset = u64::try_from(index)
                .expect("event offset fits u64")
                .checked_add(1)
                .expect("event offset increment does not overflow");
            let expected_sequence = after_sequence
                .checked_add(offset)
                .expect("durable watch event sequence does not overflow");
            assert_eq!(
                event["sequence"].as_u64(),
                Some(expected_sequence),
                "durable watch suffix is not contiguous for {monitor_id}: after={after_sequence}, target={target_cursor}, events={events:?}"
            );
        }
        Self {
            state,
            target_cursor,
            events,
        }
    }
}

struct WatchProcess {
    child: Option<Child>,
    lines: Receiver<String>,
    stdout_reader: Option<JoinHandle<Vec<u8>>>,
    stderr_reader: Option<JoinHandle<Vec<u8>>>,
}

fn read_watch_json_output(stdout: ChildStdout, line_sender: Sender<String>) -> Vec<u8> {
    let mut reader = BufReader::new(stdout);
    let mut output = Vec::new();
    loop {
        let mut line = Vec::new();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {
                output.extend_from_slice(&line);
                let text = String::from_utf8(line).expect("watch emits UTF-8 JSON lines");
                if line_sender.send(text).is_err() {
                    break;
                }
            }
            Err(error) => panic!("read watch JSON output: {error}"),
        }
    }
    output
}

impl WatchProcess {
    fn spawn(mut command: ProcessCommand) -> Self {
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the real jackin watch command");
        let stdout = child.stdout.take().expect("watch stdout is piped");
        let stderr = child.stderr.take().expect("watch stderr is piped");
        let (line_sender, lines) = mpsc::channel();
        let stdout_reader = thread::spawn(move || read_watch_json_output(stdout, line_sender));
        let stderr_reader = thread::spawn(move || {
            let mut reader = stderr;
            let mut output = Vec::new();
            reader
                .read_to_end(&mut output)
                .expect("read bounded watch stderr");
            output
        });
        Self {
            child: Some(child),
            lines,
            stdout_reader: Some(stdout_reader),
            stderr_reader: Some(stderr_reader),
        }
    }

    fn initial_events(
        &self,
        monitor_id: &str,
        expected_cursor: u64,
        expected_status: &serde_json::Value,
    ) -> Vec<serde_json::Value> {
        let mut events = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "watch did not emit its expected initial cursor"
            );
            let line = match self
                .lines
                .recv_timeout(remaining.min(Duration::from_millis(100)))
            {
                Ok(line) => line,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("watch exited before the test observed its initial cursor")
                }
            };
            let event: serde_json::Value =
                serde_json::from_str(line.trim_end()).expect("watch line is a JSON event");
            let sequence = event["sequence"]
                .as_u64()
                .expect("watch event includes its durable sequence");
            assert!(
                sequence <= expected_cursor,
                "watch advanced beyond the pre-launch initial cursor"
            );
            assert_eq!(event["status"]["monitor_id"], monitor_id);
            events.push(event);
            if sequence == expected_cursor {
                break;
            }
        }
        let initial = events
            .last()
            .expect("expected initial cursor produced a watch event");
        assert_eq!(initial["sequence"], expected_cursor);
        assert_eq!(initial["status"]["five_hour"], expected_status["five_hour"]);
        assert_eq!(initial["status"]["seven_day"], expected_status["seven_day"]);
        assert_eq!(initial["status"]["readiness"], expected_status["readiness"]);
        assert_eq!(initial["status"]["runnable"], expected_status["runnable"]);
        events
    }

    fn consume_through(
        &self,
        monitor_id: &str,
        after_sequence: u64,
        boundary: &DurableWatchBoundary,
    ) -> (u64, Vec<serde_json::Value>) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut events = Vec::with_capacity(boundary.events.len());
        for (index, expected_event) in boundary.events.iter().enumerate() {
            let event = loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(
                    !remaining.is_zero(),
                    "watch did not consume durable suffix for {monitor_id}: after={after_sequence}, target={}, received={events:?}, expected={:?}",
                    boundary.target_cursor,
                    boundary.events
                );
                let line = match self
                    .lines
                    .recv_timeout(remaining.min(Duration::from_millis(100)))
                {
                    Ok(line) => line,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        panic!(
                            "watch exited before consuming durable suffix for {monitor_id}: after={after_sequence}, target={}, received={events:?}, expected={:?}",
                            boundary.target_cursor, boundary.events
                        )
                    }
                };
                break serde_json::from_str::<serde_json::Value>(line.trim_end())
                    .expect("watch line is a JSON event");
            };
            let sequence = event["sequence"]
                .as_u64()
                .expect("watch event includes its durable sequence");
            assert_eq!(
                event["status"]["monitor_id"], monitor_id,
                "watch event changed monitor identity: event={event:?}, expected suffix={:?}",
                boundary.events
            );
            assert!(
                sequence <= boundary.target_cursor,
                "watch advanced beyond captured boundary for {monitor_id}: sequence={sequence}, target={}, event={event:?}, expected suffix={:?}",
                boundary.target_cursor,
                boundary.events
            );
            let offset = u64::try_from(index)
                .expect("received event offset fits u64")
                .checked_add(1)
                .expect("received event offset increment does not overflow");
            let expected_sequence = after_sequence
                .checked_add(offset)
                .expect("expected watch sequence does not overflow");
            assert_eq!(
                sequence, expected_sequence,
                "watch event is duplicate, regressed, or missing for {monitor_id}: after={after_sequence}, target={}, received={events:?}, actual={event:?}, expected={:?}",
                boundary.target_cursor, boundary.events
            );
            assert_eq!(
                &event, expected_event,
                "watch event differs from immutable durable event for {monitor_id} sequence={sequence}"
            );
            events.push(event);
        }
        let consumed_cursor = events
            .last()
            .and_then(|event| event["sequence"].as_u64())
            .unwrap_or(after_sequence);
        assert_eq!(
            consumed_cursor, boundary.target_cursor,
            "watch did not finish at captured durable target for {monitor_id}: after={after_sequence}, consumed={consumed_cursor}, target={}, events={events:?}",
            boundary.target_cursor
        );
        (consumed_cursor, events)
    }

    fn finish(&mut self, timeout: Duration) -> Output {
        let child = self.child.as_mut().expect("watch child remains available");
        let deadline = Instant::now() + timeout;
        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait().expect("poll watch child") {
                break status;
            }
            if Instant::now() >= deadline {
                timed_out = true;
                child.kill().expect("kill timed-out watch child");
                break child.wait().expect("reap timed-out watch child");
            }
            thread::park_timeout(Duration::from_millis(5));
        };
        self.child.take();
        let stdout = self
            .stdout_reader
            .take()
            .expect("watch stdout reader exists")
            .join()
            .expect("join watch stdout reader");
        let stderr = self
            .stderr_reader
            .take()
            .expect("watch stderr reader exists")
            .join()
            .expect("join watch stderr reader");
        assert!(!timed_out, "bounded watch exceeded its own timeout");
        Output {
            status,
            stdout,
            stderr,
        }
    }
}

impl Drop for WatchProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if child.try_wait().ok().flatten().is_none() {
                let _ignored = child.kill();
            }
            let _ignored = child.wait();
        }
        if let Some(reader) = self.stdout_reader.take() {
            let _ignored = reader.join();
        }
        if let Some(reader) = self.stderr_reader.take() {
            let _ignored = reader.join();
        }
    }
}

fn cli_binary() -> PathBuf {
    let configured = std::env::var_os("JACKIN_TEST_CLI_BIN")
        .expect("JACKIN_TEST_CLI_BIN is required when this ignored test is explicitly run");
    let configured = PathBuf::from(configured);
    assert!(
        configured.is_absolute(),
        "JACKIN_TEST_CLI_BIN must be absolute"
    );
    let path = fs::canonicalize(&configured).expect("canonicalize JACKIN_TEST_CLI_BIN");
    let metadata = fs::metadata(&path).expect("inspect JACKIN_TEST_CLI_BIN");
    assert!(
        metadata.is_file(),
        "JACKIN_TEST_CLI_BIN must name a regular file"
    );
    assert_ne!(
        metadata.mode() & 0o111,
        0,
        "JACKIN_TEST_CLI_BIN must be executable"
    );
    path
}

fn cli_base(broker: &LocalBroker, binary: &Path) -> ProcessCommand {
    broker.command(binary)
}

fn status_command(broker: &LocalBroker, binary: &Path, monitor_id: &str) -> ProcessCommand {
    let mut command = cli_base(broker, binary);
    command.args(["status", "--monitor", monitor_id]);
    command
}

fn watch_command(
    broker: &LocalBroker,
    binary: &Path,
    monitor_id: &str,
    timeout_secs: u64,
) -> ProcessCommand {
    let timeout = timeout_secs.to_string();
    let mut command = cli_base(broker, binary);
    command
        .arg("watch")
        .arg("--monitor")
        .arg(monitor_id)
        .arg("--timeout-secs")
        .arg(timeout);
    command
}

fn run_cli(mut command: ProcessCommand, timeout: Duration) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bounded jackin CLI subprocess");
    let stdout = child.stdout.take().expect("CLI stdout is piped");
    let stderr = child.stderr.take().expect("CLI stderr is piped");
    let stdout_reader = thread::spawn(move || {
        let mut output = Vec::new();
        let mut reader = stdout;
        reader
            .read_to_end(&mut output)
            .expect("read bounded CLI stdout");
        output
    });
    let stderr_reader = thread::spawn(move || {
        let mut output = Vec::new();
        let mut reader = stderr;
        reader
            .read_to_end(&mut output)
            .expect("read bounded CLI stderr");
        output
    });
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll CLI child") {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            child.kill().expect("kill timed-out CLI child");
            break child.wait().expect("reap timed-out CLI child");
        }
        thread::park_timeout(Duration::from_millis(5));
    };
    let stdout = stdout_reader.join().expect("join CLI stdout reader");
    let stderr = stderr_reader.join().expect("join CLI stderr reader");
    assert!(
        !timed_out,
        "jackin CLI subprocess exceeded its bounded timeout"
    );
    Output {
        status,
        stdout,
        stderr,
    }
}

fn json_output(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "parse CLI JSON (exit {:?}): {error}; stdout={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn monitor_state(harness: &CollectorHarness) -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(harness.monitor_state_file()).expect("read durable monitor state"),
    )
    .expect("decode durable monitor state")
}

fn capture_watch_boundary(
    harness: &CollectorHarness,
    monitor_id: &str,
    after_sequence: u64,
) -> DurableWatchBoundary {
    DurableWatchBoundary::from_state(monitor_state(harness), monitor_id, after_sequence)
}

fn assert_watch_boundary_unchanged(
    harness: &CollectorHarness,
    monitor_id: &str,
    after_sequence: u64,
    boundary: &DurableWatchBoundary,
    phase: &str,
) {
    let after = capture_watch_boundary(harness, monitor_id, after_sequence);
    assert_eq!(
        after.target_cursor, boundary.target_cursor,
        "durable watch target advanced during frozen phase {phase} for {monitor_id}: after={after_sequence}, before_target={}, before_events={:?}, after_target={}, after_events={:?}",
        boundary.target_cursor, boundary.events, after.target_cursor, after.events
    );
    assert_eq!(
        after.events, boundary.events,
        "durable watch suffix changed during frozen phase {phase} for {monitor_id}: after={after_sequence}"
    );
}

fn drain_watch_boundary(
    harness: &CollectorHarness,
    watcher: &WatchProcess,
    monitor_id: &str,
    event_cursor: &mut u64,
    phase: &str,
) -> (DurableWatchBoundary, Vec<serde_json::Value>) {
    let after_sequence = *event_cursor;
    let boundary = capture_watch_boundary(harness, monitor_id, after_sequence);
    let (consumed_cursor, events) = watcher.consume_through(monitor_id, after_sequence, &boundary);
    *event_cursor = consumed_cursor;
    assert_eq!(
        *event_cursor, boundary.target_cursor,
        "consumed watch cursor did not reach captured target after {phase}: received={events:?}, durable={:?}",
        boundary.events
    );
    assert_watch_boundary_unchanged(harness, monitor_id, after_sequence, &boundary, phase);
    (boundary, events)
}

fn exactly_one_status_event<'a>(
    events: &'a [serde_json::Value],
    phase: &str,
    predicate: impl Fn(&serde_json::Value) -> bool,
) -> &'a serde_json::Value {
    let matches = events
        .iter()
        .filter(|event| predicate(&event["status"]))
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "expected one phase status event after consuming the complete durable suffix for {phase}; events={events:?}, matching={matches:?}"
    );
    matches[0]
}

fn persisted_account<'a>(state: &'a serde_json::Value, account_id: &str) -> &'a serde_json::Value {
    &state["accounts"][account_id]
}

fn assert_persisted_used(
    observed: &serde_json::Value,
    expected_used: i32,
    expected_reset: Option<i64>,
    source_at: i64,
    received_at: i64,
) -> u64 {
    assert_eq!(observed["value"], expected_used);
    assert_eq!(
        observed["reset_at_epoch"],
        expected_reset.map_or(serde_json::Value::Null, serde_json::Value::from)
    );
    assert_eq!(observed["evidence_at_epoch"], source_at);
    assert_eq!(observed["received_at_epoch"], received_at);
    observed["input_sequence"]
        .as_u64()
        .expect("persisted used observation has an admission sequence")
}

fn assert_persisted_reset(
    observed: &serde_json::Value,
    expected_reset: i64,
    source_at: i64,
    received_at: i64,
) -> u64 {
    assert_eq!(observed["value"], expected_reset);
    assert_eq!(observed["evidence_at_epoch"], source_at);
    assert_eq!(observed["received_at_epoch"], received_at);
    observed["input_sequence"]
        .as_u64()
        .expect("persisted reset observation has an admission sequence")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionDurableIdentity {
    reset: serde_json::Value,
    latest_reset_epoch: serde_json::Value,
    reset_admission_fence: serde_json::Value,
    account_barrier: serde_json::Value,
    monitor_barrier: serde_json::Value,
}

fn session_durable_identity(
    state: &serde_json::Value,
    account_id: &str,
    monitor_id: &str,
) -> SessionDurableIdentity {
    let account = persisted_account(state, account_id);
    SessionDurableIdentity {
        reset: account["broker_windows"][0]["reset"].clone(),
        latest_reset_epoch: account["latest_reset_epochs"][0].clone(),
        reset_admission_fence: account["reset_admission_fences"][0].clone(),
        account_barrier: account["reset_barriers"][0].clone(),
        monitor_barrier: state["monitors"][monitor_id]["reset_barriers"][0].clone(),
    }
}

fn assert_session_complete_projection(
    account: &serde_json::Value,
    expected_used: i32,
    expected_reset: i64,
    source_at: i64,
    received_at: i64,
) -> u64 {
    let window = &account["broker_windows"][0];
    let used_sequence = assert_persisted_used(
        &window["used"],
        expected_used,
        Some(expected_reset),
        source_at,
        received_at,
    );
    let reset_sequence =
        assert_persisted_reset(&window["reset"], expected_reset, source_at, received_at);
    assert_eq!(used_sequence, reset_sequence);
    let pair = &window["paired"];
    assert_eq!(pair["used_percentage_basis_points"], expected_used);
    assert_eq!(pair["reset_at_epoch"], expected_reset);
    assert_eq!(pair["evidence_at_epoch"], source_at);
    assert_eq!(
        pair["source_times"],
        serde_json::json!([source_at, source_at])
    );
    assert_eq!(pair["received_at_epoch"], received_at);
    assert_eq!(pair["input_sequence"], used_sequence);
    assert_eq!(account["latest_reset_epochs"][0], expected_reset);
    let fence = &account["reset_admission_fences"][0];
    assert_eq!(fence["state"], "known");
    assert_eq!(fence["reset_at_epoch"], expected_reset);
    assert_eq!(fence["source_after_epoch"], source_at.max(received_at));
    assert_eq!(fence["receipt_after_epoch"], received_at);
    assert_eq!(fence["input_sequence"], reset_sequence);
    assert!(account["used_only_admission_sequences"][0].is_null());
    used_sequence
}

fn assert_session_orphan_projection(
    state: &serde_json::Value,
    account_id: &str,
    expected_sequence: Option<u64>,
) -> u64 {
    let account = persisted_account(state, account_id);
    let used_sequence = assert_persisted_used(
        &account["broker_windows"][0]["used"],
        0,
        None,
        NOW + 300,
        NOW + 301,
    );
    assert_eq!(account["used_only_admission_sequences"][0], used_sequence);
    assert!(account["broker_windows"][0]["paired"].is_null());
    if let Some(expected_sequence) = expected_sequence {
        assert_eq!(used_sequence, expected_sequence);
    }
    used_sequence
}

fn assert_initial_session_identity(
    state: &serde_json::Value,
    account_id: &str,
    monitor_id: &str,
) -> SessionDurableIdentity {
    let account = persisted_account(state, account_id);
    assert_session_complete_projection(account, 9_500, SESSION_RESET_EPOCH, NOW, NOW + 1);

    let identity = session_durable_identity(state, account_id, monitor_id);
    assert!(!identity.account_barrier.is_null());
    assert_eq!(identity.account_barrier["source"], "broker_projection");
    assert_eq!(
        identity.account_barrier["prior_reset_at_epoch"],
        SESSION_RESET_EPOCH
    );
    assert_eq!(
        identity.account_barrier["prior_used_percentage_basis_points"],
        9_500
    );
    assert!(!identity.monitor_barrier.is_null());
    assert_eq!(
        identity.monitor_barrier["prior_reset_at_epoch"],
        SESSION_RESET_EPOCH
    );
    assert_eq!(
        identity.monitor_barrier["prior_used_percentage_basis_points"],
        9_500
    );
    identity
}

fn assert_long_range_identity(
    state: &serde_json::Value,
    account_id: &str,
    source_at: i64,
    received_at: i64,
) {
    let account = persisted_account(state, account_id);
    let window = &account["broker_windows"][1];
    let used_sequence = assert_persisted_used(
        &window["used"],
        9_200,
        Some(LONG_RESET_EPOCH),
        source_at,
        received_at,
    );
    let reset_sequence =
        assert_persisted_reset(&window["reset"], LONG_RESET_EPOCH, source_at, received_at);
    assert_eq!(used_sequence, reset_sequence);
    let pair = &window["paired"];
    assert_eq!(pair["used_percentage_basis_points"], 9_200);
    assert_eq!(pair["reset_at_epoch"], LONG_RESET_EPOCH);
    assert_eq!(pair["evidence_at_epoch"], source_at);
    assert_eq!(
        pair["source_times"],
        serde_json::json!([source_at, source_at])
    );
    assert_eq!(pair["received_at_epoch"], received_at);
    assert_eq!(pair["input_sequence"], used_sequence);
    assert_eq!(account["latest_reset_epochs"][1], LONG_RESET_EPOCH);
    let fence = &account["reset_admission_fences"][1];
    assert_eq!(fence["state"], "known");
    assert_eq!(fence["reset_at_epoch"], LONG_RESET_EPOCH);
    assert_eq!(fence["source_after_epoch"], source_at.max(received_at));
    assert_eq!(fence["receipt_after_epoch"], received_at);
    assert_eq!(fence["input_sequence"], reset_sequence);
    assert!(account["used_only_admission_sequences"][1].is_null());
}

fn release_expected_probe(
    harness: &CollectorHarness,
    executor: &ScriptedExecutor,
    started: &Receiver<ProbeCall>,
    releases: &[Sender<()>],
    expected_generation: u64,
    epoch: i64,
) {
    let call = started
        .recv_timeout(PROBE_START_TIMEOUT)
        .expect("coordinator starts the scheduled provider generation");
    assert_eq!(call.account_id, harness.capability.account_id);
    assert_eq!(call.surface_id, harness.capability.surface_id);
    assert_eq!(call.generation, expected_generation);
    let expected_count = usize::try_from(expected_generation)
        .expect("provider generation fits the exact call counter");
    assert_calls(executor, &harness.capability, expected_count);
    releases[expected_count - 1]
        .send(())
        .expect("release the expected provider generation after its collection tick");
    wait_for_probe_terminal(harness, executor, expected_count, epoch);
}

fn wait_for_probe_terminal(
    harness: &CollectorHarness,
    executor: &ScriptedExecutor,
    count: usize,
    epoch: i64,
) {
    let deadline = Instant::now() + Duration::from_secs(3);
    let expected_generation = u64::try_from(count).expect("provider call count fits u64");
    loop {
        assert!(
            Instant::now() < deadline,
            "provider generation {expected_generation} did not reach terminal state"
        );
        if executor.call_count() == count
            && harness
                .coordinator
                .current(&harness.capability, epoch)
                .is_ok_and(|view| {
                    view.generation == expected_generation && view.phase.is_terminal()
                })
        {
            return;
        }
        thread::park_timeout(Duration::from_millis(5));
    }
}

fn assert_calls(executor: &ScriptedExecutor, capability: &UsageAccountCapability, expected: usize) {
    assert_eq!(executor.call_count(), expected);
    let requested = executor.requested();
    assert_eq!(requested.len(), expected);
    assert!(requested.iter().all(|call| {
        call.account_id == capability.account_id && call.surface_id == capability.surface_id
    }));
    assert_eq!(
        requested
            .iter()
            .map(|call| call.generation)
            .collect::<Vec<_>>(),
        (1..=u64::try_from(expected).expect("call count fits u64")).collect::<Vec<_>>()
    );
}

fn wait_for_accept_count(broker: &LocalBroker, expected: usize) {
    broker.wait_for_accepts(expected);
}

fn assert_status_blocked(status: &serde_json::Value, expected_used: i64, expected_reset: i64) {
    assert_eq!(
        status["five_hour"]["used_percentage_basis_points"],
        expected_used
    );
    assert_eq!(status["five_hour"]["reset_at_epoch"], expected_reset);
    assert_eq!(status["readiness"]["quota"], "unknown");
    assert_eq!(status["readiness"]["dispatch"], "blocked");
    assert_eq!(status["runnable"], false);
    assert!(
        status["issues"]
            .as_array()
            .is_some_and(|issues| { issues.iter().any(|issue| issue["code"] == "missing_reset") })
    );
}

fn assert_monitor_authority_config(
    state: &serde_json::Value,
    monitor_id: &str,
    binding: &MonitorAccountBinding,
    expected_purpose: &str,
    expected_collector: bool,
) {
    let monitor = &state["monitors"][monitor_id];
    let config = &monitor["config"];
    assert_eq!(config["purpose"], expected_purpose);
    assert_eq!(config["experimental_collector"], expected_collector);
    assert_eq!(config["scope"]["scope"], "bound_account");
    assert_eq!(config["scope"]["binding_id"], binding.binding_id);
    assert_eq!(config["scope"]["binding_revision"], binding.revision);
    assert_eq!(monitor["account_id"], binding.account_id);
    if expected_purpose == "observe_only" {
        assert!(config["goal_id"].is_null());
        assert!(config["policy_revision"].is_null());
    }
}

fn assert_observer_not_dispatchable(
    status: &serde_json::Value,
    monitor_id: &str,
    expected_used: i64,
    expected_reset: i64,
    quota_ready: bool,
) {
    assert_eq!(status["monitor_id"], monitor_id);
    assert_eq!(status["purpose"], "observe_only");
    assert_eq!(
        status["five_hour"]["used_percentage_basis_points"],
        expected_used
    );
    assert_eq!(status["five_hour"]["reset_at_epoch"], expected_reset);
    assert_eq!(status["readiness"]["dispatch"], "not_authorized");
    assert_eq!(status["runnable"], false);
    if quota_ready {
        assert_eq!(status["readiness"]["quota"], "ready");
    }
}

fn assert_used_field_identity(
    status: &serde_json::Value,
    window_name: &str,
    expected_used: i64,
    source_at: i64,
    received_at: i64,
    account_id: &str,
) {
    let window = &status[window_name];
    let field = &window["used_evidence"];
    assert_eq!(field["freshness"], "current");
    assert_eq!(field["evidence_at_epoch"], source_at);
    assert_eq!(field["evidence_received_at_epoch"], received_at);
    let evidence_sequence = field["evidence_sequence"]
        .as_u64()
        .expect("used evidence includes its monitor event sequence");
    let matching = status["evidence"]
        .as_array()
        .expect("status includes retained evidence")
        .iter()
        .filter(|evidence| {
            evidence["sequence"] == evidence_sequence
                && evidence["source"] == "broker_projection"
                && evidence["account_id"] == account_id
                && evidence["session_id"].is_null()
                && evidence["evidence_at_epoch"] == source_at
                && evidence["evidence_received_at_epoch"] == received_at
                && evidence["value"]["kind"] == "quota_used_percentage"
                && evidence["value"]["used_percentage_basis_points"] == expected_used
        })
        .count();
    assert_eq!(
        matching, 1,
        "stored evidence identity must match the selected field"
    );
}

fn assert_real_clock_precedes_fixture() {
    let real_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_secs();
    let real_epoch = i64::try_from(real_epoch).expect("system clock epoch fits i64");
    assert!(
        real_epoch < NOW,
        "the socket CLI reads real UTC; this fixture requires it to remain below its fixed NOW epoch"
    );
}

struct ResetlessCliStart<'a> {
    binary: &'a Path,
    executor: &'a ScriptedExecutor,
    probe_starts: &'a Receiver<ProbeCall>,
    probe_releases: &'a [Sender<()>],
    harness: &'a CollectorHarness,
    binding: &'a MonitorAccountBinding,
    monitor_id: &'a String,
    observer_monitor_id: &'a String,
    broker: &'a LocalBroker,
    ticker: &'a FakeTicker,
}

struct ResetlessCliScenario<'a> {
    start: &'a ResetlessCliStart<'a>,
    watcher: &'a mut WatchProcess,
    event_cursor: &'a mut u64,
    consumed_event_ledger: &'a mut Vec<serde_json::Value>,
    initial_reply: &'a serde_json::Value,
    initial_state: &'a serde_json::Value,
    initial_session_identity: &'a SessionDurableIdentity,
}
fn start_initial_generation(
    start: &ResetlessCliStart<'_>,
) -> (serde_json::Value, serde_json::Value, SessionDurableIdentity) {
    let binary = start.binary;
    let executor = start.executor;
    let probe_starts = start.probe_starts;
    let probe_releases = start.probe_releases;
    let harness = start.harness;
    let binding = start.binding;
    let monitor_id = start.monitor_id;
    let observer_monitor_id = start.observer_monitor_id;
    let broker = start.broker;
    let ticker = start.ticker;

    ticker.tick(harness, NOW);
    release_expected_probe(harness, executor, probe_starts, probe_releases, 1, NOW);
    ticker.tick(harness, NOW + 1);
    assert_calls(executor, &harness.capability, 1);

    let initial_output = run_cli(
        status_command(broker, binary, monitor_id),
        Duration::from_secs(5),
    );
    assert_eq!(initial_output.status.code(), Some(2));
    let initial_reply = json_output(&initial_output);
    let initial_status = &initial_reply["status"];
    assert_eq!(initial_reply["result"], "status");
    assert_eq!(
        initial_status["five_hour"]["used_percentage_basis_points"],
        9_500
    );
    assert_eq!(
        initial_status["five_hour"]["reset_at_epoch"],
        SESSION_RESET_EPOCH
    );
    assert_eq!(
        initial_status["seven_day"]["used_percentage_basis_points"],
        9_200
    );
    assert_eq!(
        initial_status["seven_day"]["reset_at_epoch"],
        LONG_RESET_EPOCH
    );
    let initial_observer_output = run_cli(
        status_command(broker, binary, observer_monitor_id),
        Duration::from_secs(5),
    );
    assert_eq!(initial_observer_output.status.code(), Some(2));
    let initial_observer_reply = json_output(&initial_observer_output);
    assert_eq!(initial_observer_reply["result"], "status");
    assert_observer_not_dispatchable(
        &initial_observer_reply["status"],
        observer_monitor_id,
        9_500,
        SESSION_RESET_EPOCH,
        false,
    );
    assert_calls(executor, &harness.capability, 1);
    assert_used_field_identity(
        initial_status,
        "five_hour",
        9_500,
        NOW,
        NOW + 1,
        &binding.account_id,
    );
    let initial_state = monitor_state(harness);
    assert_monitor_authority_config(&initial_state, monitor_id, binding, "dispatch_guard", false);
    assert_monitor_authority_config(
        &initial_state,
        observer_monitor_id,
        binding,
        "observe_only",
        true,
    );
    let initial_session_identity =
        assert_initial_session_identity(&initial_state, &binding.account_id, monitor_id);
    assert_long_range_identity(&initial_state, &binding.account_id, NOW, NOW + 1);
    (initial_reply, initial_state, initial_session_identity)
}
fn publish_generation_two(scenario: &mut ResetlessCliScenario<'_>) -> u64 {
    let start = scenario.start;
    let ticker = start.ticker;
    let harness = start.harness;
    let executor = start.executor;
    let probe_starts = start.probe_starts;
    let probe_releases = start.probe_releases;
    let watcher = &mut *scenario.watcher;
    let monitor_id = start.monitor_id;
    let event_cursor = &mut *scenario.event_cursor;
    let consumed_event_ledger = &mut *scenario.consumed_event_ledger;
    let binding = start.binding;
    let initial_state = scenario.initial_state;
    let initial_session_identity = scenario.initial_session_identity;

    ticker.tick(harness, NOW + 299);
    assert_calls(executor, &harness.capability, 1);
    let (_, events) =
        drain_watch_boundary(harness, watcher, monitor_id, event_cursor, "sample NOW+299");
    consumed_event_ledger.extend(events);
    ticker.tick(harness, NOW + 300);
    release_expected_probe(
        harness,
        executor,
        probe_starts,
        probe_releases,
        2,
        NOW + 300,
    );
    assert_calls(executor, &harness.capability, 2);
    let (_, events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "generation 2 publication at NOW+300",
    );
    consumed_event_ledger.extend(events);

    ticker.tick(harness, NOW + 301);
    let (resetless_boundary, resetless_events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "resetless publication at NOW+301",
    );
    consumed_event_ledger.extend(resetless_events.iter().cloned());
    let resetless_event = exactly_one_status_event(
        &resetless_events,
        "resetless publication at NOW+301",
        |status| {
            status["five_hour"]["used_percentage_basis_points"] == 0
                && status["five_hour"]["used_evidence"]["evidence_at_epoch"] == NOW + 300
                && status["five_hour"]["used_evidence"]["evidence_received_at_epoch"] == NOW + 301
        },
    );
    let resetless_status = &resetless_event["status"];
    assert_status_blocked(resetless_status, 0, SESSION_RESET_EPOCH);
    assert_eq!(resetless_status["five_hour"]["reset_validity"], "due");
    assert_used_field_identity(
        resetless_status,
        "five_hour",
        0,
        NOW + 300,
        NOW + 301,
        &binding.account_id,
    );
    assert_eq!(
        resetless_status["seven_day"]["used_percentage_basis_points"],
        9_200
    );
    assert_eq!(resetless_status["seven_day"]["reset_validity"], "future");
    assert_used_field_identity(
        resetless_status,
        "seven_day",
        9_200,
        NOW + 300,
        NOW + 301,
        &binding.account_id,
    );

    let resetless_state = &resetless_boundary.state;
    assert_eq!(
        session_durable_identity(resetless_state, &binding.account_id, monitor_id),
        *initial_session_identity,
        "resetless used-only evidence must retain the original Session reset, high-water, fence and complete barriers"
    );
    let orphan_sequence =
        assert_session_orphan_projection(resetless_state, &binding.account_id, None);
    let initial_session_used_sequence = initial_state["accounts"][binding.account_id.as_str()]
        ["broker_windows"][0]["used"]["input_sequence"]
        .as_u64()
        .expect("initial complete Session projection has an input sequence");
    assert!(orphan_sequence > initial_session_used_sequence);
    assert_long_range_identity(resetless_state, &binding.account_id, NOW + 300, NOW + 301);
    orphan_sequence
}

fn assert_sample_302_and_cli_status(scenario: &mut ResetlessCliScenario<'_>, orphan_sequence: u64) {
    let start = scenario.start;
    let ticker = start.ticker;
    let harness = start.harness;
    let watcher = &mut *scenario.watcher;
    let monitor_id = start.monitor_id;
    let event_cursor = &mut *scenario.event_cursor;
    let consumed_event_ledger = &mut *scenario.consumed_event_ledger;
    let binding = start.binding;
    let initial_session_identity = scenario.initial_session_identity;
    let broker = start.broker;
    let binary = start.binary;

    ticker.tick(harness, NOW + 302);
    let (sample_302_boundary, events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "sample NOW+302 before CLI status",
    );
    consumed_event_ledger.extend(events);
    assert_eq!(
        session_durable_identity(&sample_302_boundary.state, &binding.account_id, monitor_id),
        *initial_session_identity,
        "sample NOW+302 preserves Session reset, high-water, fence and barriers"
    );
    assert_session_orphan_projection(
        &sample_302_boundary.state,
        &binding.account_id,
        Some(orphan_sequence),
    );
    assert_long_range_identity(
        &sample_302_boundary.state,
        &binding.account_id,
        NOW + 300,
        NOW + 301,
    );
    let published_status = run_cli(
        status_command(broker, binary, monitor_id),
        Duration::from_secs(5),
    );
    assert_eq!(published_status.status.code(), Some(2));
    let published_status = json_output(&published_status);
    assert_status_blocked(&published_status["status"], 0, SESSION_RESET_EPOCH);
    assert_eq!(
        published_status["status"]["five_hour"]["reset_validity"],
        "due"
    );
    let (published_boundary, events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "CLI status after sample NOW+302",
    );
    consumed_event_ledger.extend(events);
    let published_state = &published_boundary.state;
    assert_eq!(
        session_durable_identity(published_state, &binding.account_id, monitor_id),
        *initial_session_identity
    );
    assert_session_orphan_projection(published_state, &binding.account_id, Some(orphan_sequence));
    assert_long_range_identity(published_state, &binding.account_id, NOW + 300, NOW + 301);
}

fn assert_grace_checkpoint(scenario: &mut ResetlessCliScenario<'_>, orphan_sequence: u64) {
    let start = scenario.start;
    let ticker = start.ticker;
    let harness = start.harness;
    let executor = start.executor;
    let watcher = &mut *scenario.watcher;
    let monitor_id = start.monitor_id;
    let event_cursor = &mut *scenario.event_cursor;
    let consumed_event_ledger = &mut *scenario.consumed_event_ledger;
    let binding = start.binding;
    let initial_session_identity = scenario.initial_session_identity;
    let broker = start.broker;
    let binary = start.binary;

    ticker.tick(harness, NOW + 400);
    assert_calls(executor, &harness.capability, 2);
    let (due_boundary, due_events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "grace checkpoint at NOW+400",
    );
    consumed_event_ledger.extend(due_events.iter().cloned());
    let due_event =
        exactly_one_status_event(&due_events, "grace checkpoint at NOW+400", |status| {
            status["five_hour"]["used_percentage_basis_points"] == 0
                && status["issues"].as_array().is_some_and(|issues| {
                    issues
                        .iter()
                        .any(|issue| issue["code"] == "reset_due_unverified")
                })
        });
    assert_status_blocked(&due_event["status"], 0, SESSION_RESET_EPOCH);
    assert_eq!(
        session_durable_identity(&due_boundary.state, &binding.account_id, monitor_id),
        *initial_session_identity
    );
    assert_session_orphan_projection(
        &due_boundary.state,
        &binding.account_id,
        Some(orphan_sequence),
    );
    assert_long_range_identity(
        &due_boundary.state,
        &binding.account_id,
        NOW + 300,
        NOW + 301,
    );
    let cooldown_status = run_cli(
        status_command(broker, binary, monitor_id),
        Duration::from_secs(5),
    );
    assert_eq!(cooldown_status.status.code(), Some(2));
    assert_status_blocked(
        &json_output(&cooldown_status)["status"],
        0,
        SESSION_RESET_EPOCH,
    );
    let (cooldown_status_boundary, events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "cooldown CLI status",
    );
    consumed_event_ledger.extend(events);
    assert_eq!(
        session_durable_identity(
            &cooldown_status_boundary.state,
            &binding.account_id,
            monitor_id,
        ),
        *initial_session_identity,
        "cooldown status preserves every original Session reset/barrier identity"
    );
    assert_session_orphan_projection(
        &cooldown_status_boundary.state,
        &binding.account_id,
        Some(orphan_sequence),
    );
    assert_long_range_identity(
        &cooldown_status_boundary.state,
        &binding.account_id,
        NOW + 300,
        NOW + 301,
    );
    let cooldown_watch = run_cli(
        watch_command(broker, binary, monitor_id, 1),
        Duration::from_secs(3),
    );
    assert_eq!(cooldown_watch.status.code(), Some(0));
    let cooldown_events = cooldown_watch
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).expect("watch JSON event"))
        .collect::<Vec<_>>();
    assert!(!cooldown_events.is_empty());
    let (cooldown_watch_boundary, events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "cooldown short watch",
    );
    consumed_event_ledger.extend(events);
    let durable_latest_cooldown_event = cooldown_watch_boundary.state["monitors"]
        [monitor_id.as_str()]["events"]
        .as_array()
        .and_then(|events| events.last())
        .expect("cooldown durable state retains the current watch event");
    assert_eq!(
        cooldown_events.last(),
        Some(durable_latest_cooldown_event),
        "short cooldown watch returns the exact latest durable event"
    );
    assert_eq!(
        cooldown_events
            .iter()
            .filter_map(|event| event["sequence"].as_u64())
            .max(),
        Some(*event_cursor)
    );
    assert_calls(executor, &harness.capability, 2);
    let cooldown_state = &cooldown_watch_boundary.state;
    assert_eq!(
        session_durable_identity(cooldown_state, &binding.account_id, monitor_id),
        *initial_session_identity,
        "cooldown status and watch reads preserve every original Session reset/barrier identity"
    );
    assert_session_orphan_projection(cooldown_state, &binding.account_id, Some(orphan_sequence));
    assert_long_range_identity(cooldown_state, &binding.account_id, NOW + 300, NOW + 301);
}

fn publish_old_reset(
    scenario: &mut ResetlessCliScenario<'_>,
) -> (DurableWatchBoundary, Vec<serde_json::Value>) {
    let start = scenario.start;
    let ticker = start.ticker;
    let harness = start.harness;
    let executor = start.executor;
    let probe_starts = start.probe_starts;
    let probe_releases = start.probe_releases;
    let watcher = &mut *scenario.watcher;
    let monitor_id = start.monitor_id;
    let event_cursor = &mut *scenario.event_cursor;
    let consumed_event_ledger = &mut *scenario.consumed_event_ledger;

    ticker.tick(harness, NOW + 600);
    release_expected_probe(
        harness,
        executor,
        probe_starts,
        probe_releases,
        3,
        NOW + 600,
    );
    assert_calls(executor, &harness.capability, 3);
    let (_, events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "generation 3 publication at NOW+600",
    );
    consumed_event_ledger.extend(events);
    ticker.tick(harness, NOW + 601);
    let (old_reset_boundary, old_reset_events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "old-reset publication at NOW+601",
    );
    consumed_event_ledger.extend(old_reset_events.iter().cloned());
    (old_reset_boundary, old_reset_events)
}

fn assert_old_reset(
    scenario: &mut ResetlessCliScenario<'_>,
    old_reset_boundary: &DurableWatchBoundary,
    old_reset_events: &[serde_json::Value],
) -> SessionDurableIdentity {
    let start = scenario.start;
    let binding = start.binding;
    let monitor_id = start.monitor_id;
    let initial_session_identity = scenario.initial_session_identity;

    let old_reset_event = exactly_one_status_event(
        old_reset_events,
        "old-reset publication at NOW+601",
        |status| {
            status["five_hour"]["used_percentage_basis_points"] == 2_000
                && status["five_hour"]["reset_at_epoch"] == SESSION_RESET_EPOCH
                && status["five_hour"]["used_evidence"]["evidence_at_epoch"] == NOW + 600
                && status["five_hour"]["used_evidence"]["evidence_received_at_epoch"] == NOW + 601
        },
    );
    let old_reset_status = &old_reset_event["status"];
    let old_reset_state = &old_reset_boundary.state;
    let old_reset_account = persisted_account(old_reset_state, &binding.account_id);
    let old_reset_identity =
        session_durable_identity(old_reset_state, &binding.account_id, monitor_id);
    let old_reset_context = format!(
        "old-reset NOW+601: target_cursor={} events={:?} selected_event={:?} session_window={:?} long_range_window={:?} session_identity={:?} long_range_account_barrier={:?} long_range_monitor_barrier={:?}",
        old_reset_boundary.target_cursor,
        old_reset_boundary.events,
        old_reset_event,
        old_reset_account["broker_windows"][0],
        old_reset_account["broker_windows"][1],
        old_reset_identity,
        old_reset_account["reset_barriers"][1],
        old_reset_state["monitors"][monitor_id.as_str()]["reset_barriers"][1],
    );
    assert_eq!(
        old_reset_status["five_hour"]["reset_at_epoch"],
        SESSION_RESET_EPOCH
    );
    assert_eq!(
        old_reset_status["five_hour"]["reset_validity"], "due",
        "{old_reset_context}"
    );
    assert_eq!(
        old_reset_status["readiness"]["quota"], "stale",
        "{old_reset_context}"
    );
    assert_eq!(
        old_reset_status["readiness"]["dispatch"], "blocked",
        "{old_reset_context}"
    );
    assert_eq!(old_reset_status["runnable"], false, "{old_reset_context}");
    assert_eq!(
        old_reset_status["five_hour"]["reset_evidence"]["freshness"], "current",
        "{old_reset_context}"
    );
    assert_eq!(
        old_reset_status["seven_day"]["reset_validity"], "future",
        "{old_reset_context}"
    );
    assert_eq!(
        old_reset_status["seven_day"]["reset_evidence"]["freshness"], "current",
        "{old_reset_context}"
    );
    assert_used_field_identity(
        old_reset_status,
        "five_hour",
        2_000,
        NOW + 600,
        NOW + 601,
        &binding.account_id,
    );
    assert_eq!(
        old_reset_status["seven_day"]["used_percentage_basis_points"],
        9_200
    );
    assert_used_field_identity(
        old_reset_status,
        "seven_day",
        9_200,
        NOW + 600,
        NOW + 601,
        &binding.account_id,
    );
    let old_reset_issues = old_reset_status["issues"].as_array().unwrap();
    assert!(
        !old_reset_issues
            .iter()
            .any(|issue| issue["code"] == "quota_stale"),
        "{old_reset_context}"
    );
    assert!(
        old_reset_status["latest_decision"]["actions"]
            .as_array()
            .is_some_and(|actions| {
                actions
                    .iter()
                    .any(|action| action["action"] == "pause" && action["reason"] == "quota_stale")
            }),
        "{old_reset_context}"
    );
    assert!(
        old_reset_issues
            .iter()
            .any(|issue| issue["code"] == "reset_due_unverified"),
        "{old_reset_context}"
    );
    assert!(
        !old_reset_issues
            .iter()
            .any(|issue| issue["code"] == "missing_reset"),
        "{old_reset_context}"
    );
    assert_session_complete_projection(
        old_reset_account,
        2_000,
        SESSION_RESET_EPOCH,
        NOW + 600,
        NOW + 601,
    );
    assert_long_range_identity(old_reset_state, &binding.account_id, NOW + 600, NOW + 601);
    assert_eq!(
        old_reset_identity.latest_reset_epoch,
        initial_session_identity.latest_reset_epoch
    );
    assert_eq!(
        old_reset_identity.account_barrier, initial_session_identity.account_barrier,
        "the same-reset complete response cannot release or rewrite the account barrier"
    );
    assert_eq!(
        old_reset_identity.monitor_barrier, initial_session_identity.monitor_barrier,
        "the same-reset complete response cannot release or rewrite the monitor barrier"
    );
    old_reset_identity
}

fn refresh_recovery_model(
    scenario: &mut ResetlessCliScenario<'_>,
    old_reset_identity: &SessionDurableIdentity,
) {
    let start = scenario.start;
    let harness = start.harness;
    let binding = start.binding;
    let watcher = &mut *scenario.watcher;
    let monitor_id = start.monitor_id;
    let event_cursor = &mut *scenario.event_cursor;
    let consumed_event_ledger = &mut *scenario.consumed_event_ledger;
    let initial_status = &scenario.initial_reply["status"];

    refresh_model_for_recovery(harness, binding);
    let (model_refresh_boundary, model_refresh_events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "statusline model refresh before recovery spend receipt",
    );
    consumed_event_ledger.extend(model_refresh_events.iter().cloned());
    let model_refresh_event = model_refresh_events
        .last()
        .expect("fresh recovery model publishes a durable guard status");
    let model_refresh_status = &model_refresh_event["status"];
    assert_eq!(model_refresh_status["model"], "claude-sonnet-4-5");
    assert_eq!(model_refresh_status["model_guard_validity"], "match");
    assert_eq!(
        model_refresh_status["model_evidence"]["freshness"],
        "current"
    );
    assert!(model_refresh_status["model_evidence"]["evidence_at_epoch"].is_null());
    assert_eq!(
        model_refresh_status["model_evidence"]["evidence_received_at_epoch"],
        NOW + 900
    );
    let fresh_model_evidence = model_refresh_status["evidence"]
        .as_array()
        .expect("recovery status retains model evidence")
        .iter()
        .find(|evidence| {
            evidence["source"] == "statusline"
                && evidence["session_id"] == "resetless-cli-recovery-model"
                && evidence["value"]["kind"] == "model"
                && evidence["value"]["model"] == "claude-sonnet-4-5"
                && evidence["evidence_received_at_epoch"] == NOW + 900
        })
        .expect("status exposes the fresh matching model from the new recovery session");
    assert!(fresh_model_evidence["evidence_at_epoch"].is_null());
    assert_eq!(
        model_refresh_status["five_hour"]["reset_at_epoch"],
        SESSION_RESET_EPOCH
    );
    assert_eq!(model_refresh_status["five_hour"]["reset_validity"], "due");
    assert_eq!(model_refresh_status["readiness"]["quota"], "stale");
    assert_eq!(model_refresh_status["readiness"]["dispatch"], "blocked");
    assert_eq!(model_refresh_status["runnable"], false);
    assert_eq!(
        model_refresh_status["cumulative_goal_spend"], initial_status["cumulative_goal_spend"],
        "fresh model evidence does not change cumulative spend"
    );
    assert_eq!(
        model_refresh_status["spend_period_baseline"], initial_status["spend_period_baseline"],
        "fresh model evidence preserves the approved period baseline"
    );
    let model_refresh_state = &model_refresh_boundary.state;
    assert_eq!(
        session_durable_identity(model_refresh_state, &binding.account_id, monitor_id),
        *old_reset_identity,
        "fresh model evidence cannot release either Session reset barrier"
    );
    assert_session_complete_projection(
        persisted_account(model_refresh_state, &binding.account_id),
        2_000,
        SESSION_RESET_EPOCH,
        NOW + 600,
        NOW + 601,
    );
    assert_long_range_identity(
        model_refresh_state,
        &binding.account_id,
        NOW + 600,
        NOW + 601,
    );
}

fn record_recovery_spend(scenario: &mut ResetlessCliScenario<'_>) {
    let start = scenario.start;
    let harness = start.harness;
    let binding = start.binding;

    let recovery_spend = match harness
        .store
        .operate(
            MonitorOperation::RecordSpend {
                record: SpendRecordInput {
                    account_id: binding.account_id.clone(),
                    billing_period_start_epoch: NOW - 100,
                    billing_period_end_epoch: NOW + 100_000,
                    amount: Money::new(0, "SGD", 2),
                    evidence_at_epoch: Some(NOW + 900),
                    verified: true,
                    source: SpendRecordSource::OperatorReceipt,
                },
            },
            NOW + 900,
        )
        .expect("record a fresh verified same-period spend receipt for recovery")
    {
        MonitorReply::SpendRecorded { record } => record,
        other => panic!("expected spend-recorded reply, got {other:?}"),
    };
    assert_eq!(recovery_spend.account_id, binding.account_id);
    assert_eq!(recovery_spend.billing_period_start_epoch, NOW - 100);
    assert_eq!(recovery_spend.billing_period_end_epoch, NOW + 100_000);
    assert_eq!(recovery_spend.amount, Money::new(0, "SGD", 2));
    assert_eq!(recovery_spend.evidence_at_epoch, Some(NOW + 900));
    assert_eq!(recovery_spend.evidence_received_at_epoch, NOW + 900);
    assert_eq!(recovery_spend.source, SpendRecordSource::OperatorReceipt);
    assert_eq!(
        recovery_spend.verification,
        jackin_protocol::usage_monitor::SpendVerification::Verified
    );
}

fn assert_recovery_spend_refresh(
    scenario: &mut ResetlessCliScenario<'_>,
    old_reset_identity: &SessionDurableIdentity,
) {
    let start = scenario.start;
    let harness = start.harness;
    let binding = start.binding;
    let watcher = &mut *scenario.watcher;
    let monitor_id = start.monitor_id;
    let event_cursor = &mut *scenario.event_cursor;
    let consumed_event_ledger = &mut *scenario.consumed_event_ledger;
    let initial_status = &scenario.initial_reply["status"];

    let (spend_refresh_boundary, spend_refresh_events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "operator spend receipt refresh before generation 4",
    );
    consumed_event_ledger.extend(spend_refresh_events.iter().cloned());
    let spend_refresh_event = spend_refresh_events
        .last()
        .expect("fresh recovery spend receipt publishes a durable guard status");
    let spend_refresh_status = &spend_refresh_event["status"];
    assert_eq!(spend_refresh_status["readiness"]["budget"], "verified");
    assert_eq!(spend_refresh_status["readiness"]["quota"], "stale");
    assert_eq!(spend_refresh_status["readiness"]["dispatch"], "blocked");
    assert_eq!(spend_refresh_status["runnable"], false);
    assert_eq!(
        spend_refresh_status["five_hour"]["reset_at_epoch"],
        SESSION_RESET_EPOCH
    );
    assert_eq!(spend_refresh_status["five_hour"]["reset_validity"], "due");
    assert_eq!(spend_refresh_status["model_guard_validity"], "match");
    assert_eq!(
        spend_refresh_status["model_evidence"]["freshness"],
        "current"
    );
    assert!(spend_refresh_status["model_evidence"]["evidence_at_epoch"].is_null());
    assert_eq!(
        spend_refresh_status["model_evidence"]["evidence_received_at_epoch"],
        NOW + 900
    );
    assert_eq!(
        spend_refresh_status["cumulative_goal_spend"], initial_status["cumulative_goal_spend"],
        "fresh zero spend receipt preserves cumulative spend"
    );
    assert_eq!(
        spend_refresh_status["spend_period_baseline"]["account_id"],
        initial_status["spend_period_baseline"]["account_id"]
    );
    assert_eq!(
        spend_refresh_status["spend_period_baseline"]["billing_period_start_epoch"],
        initial_status["spend_period_baseline"]["billing_period_start_epoch"]
    );
    assert_eq!(
        spend_refresh_status["spend_period_baseline"]["billing_period_end_epoch"],
        initial_status["spend_period_baseline"]["billing_period_end_epoch"]
    );
    assert_eq!(
        spend_refresh_status["spend_period_baseline"]["amount"],
        initial_status["spend_period_baseline"]["amount"],
        "fresh zero spend receipt preserves the approved baseline amount and currency"
    );
    assert_eq!(
        spend_refresh_status["spend_period_baseline"]["amount"]["amount_minor"],
        0
    );
    assert_eq!(
        spend_refresh_status["spend_period_baseline"]["amount"]["currency"],
        "SGD"
    );
    assert_eq!(
        spend_refresh_status["spend_period_baseline"]["amount"]["exponent"],
        2
    );
    assert_eq!(
        spend_refresh_status["spend_period_baseline"]["verification"],
        "verified"
    );
    assert_eq!(
        spend_refresh_status["spend_period_baseline"]["evidence_at_epoch"],
        NOW + 900
    );
    assert_eq!(
        spend_refresh_status["spend_period_baseline"]["evidence_received_at_epoch"],
        NOW + 900
    );
    let spend_evidence = spend_refresh_status["evidence"]
        .as_array()
        .expect("recovery status retains spend evidence")
        .iter()
        .find(|evidence| {
            evidence["account_id"] == binding.account_id
                && evidence["source"] == "operator"
                && evidence["evidence_at_epoch"] == NOW + 900
                && evidence["evidence_received_at_epoch"] == NOW + 900
                && evidence["value"]["kind"] == "spend"
        })
        .expect("status exposes the exact verified recovery spend receipt");
    assert_eq!(spend_evidence["session_id"], serde_json::Value::Null);
    assert_eq!(spend_evidence["value"]["amount"]["amount_minor"], 0);
    assert_eq!(spend_evidence["value"]["amount"]["currency"], "SGD");
    assert_eq!(spend_evidence["value"]["amount"]["exponent"], 2);
    assert_eq!(
        spend_evidence["value"]["billing_period_start_epoch"],
        NOW - 100
    );
    assert_eq!(
        spend_evidence["value"]["billing_period_end_epoch"],
        NOW + 100_000
    );
    assert_eq!(spend_evidence["value"]["verification"], "verified");
    let spend_refresh_state = &spend_refresh_boundary.state;
    assert_eq!(
        session_durable_identity(spend_refresh_state, &binding.account_id, monitor_id),
        *old_reset_identity,
        "fresh model and spend evidence cannot release either Session reset barrier"
    );
    assert_session_complete_projection(
        persisted_account(spend_refresh_state, &binding.account_id),
        2_000,
        SESSION_RESET_EPOCH,
        NOW + 600,
        NOW + 601,
    );
    assert_long_range_identity(
        spend_refresh_state,
        &binding.account_id,
        NOW + 600,
        NOW + 601,
    );
}

fn publish_paired_recovery(
    scenario: &mut ResetlessCliScenario<'_>,
) -> (DurableWatchBoundary, Vec<serde_json::Value>) {
    let start = scenario.start;
    let ticker = start.ticker;
    let harness = start.harness;
    let executor = start.executor;
    let probe_starts = start.probe_starts;
    let probe_releases = start.probe_releases;
    let watcher = &mut *scenario.watcher;
    let monitor_id = start.monitor_id;
    let event_cursor = &mut *scenario.event_cursor;
    let consumed_event_ledger = &mut *scenario.consumed_event_ledger;

    ticker.tick(harness, NOW + 900);
    release_expected_probe(
        harness,
        executor,
        probe_starts,
        probe_releases,
        4,
        NOW + 900,
    );
    assert_calls(executor, &harness.capability, 4);
    let (_, events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "generation 4 publication at NOW+900",
    );
    consumed_event_ledger.extend(events);
    ticker.tick(harness, NOW + 901);
    let (recovered_boundary, recovered_events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "advanced paired recovery at NOW+901",
    );
    consumed_event_ledger.extend(recovered_events.iter().cloned());
    (recovered_boundary, recovered_events)
}

fn assert_paired_recovery<'events>(
    scenario: &mut ResetlessCliScenario<'_>,
    recovered_boundary: &DurableWatchBoundary,
    recovered_events: &'events [serde_json::Value],
) -> &'events serde_json::Value {
    let start = scenario.start;
    let binding = start.binding;
    let monitor_id = start.monitor_id;

    let recovered_event = exactly_one_status_event(
        recovered_events,
        "advanced paired recovery at NOW+901",
        |status| {
            status["five_hour"]["used_percentage_basis_points"] == 2_000
                && status["five_hour"]["reset_at_epoch"] == NOW + 4_000
                && status["five_hour"]["used_evidence"]["evidence_at_epoch"] == NOW + 900
                && status["five_hour"]["used_evidence"]["evidence_received_at_epoch"] == NOW + 901
        },
    );
    let recovered_status = &recovered_event["status"];
    assert_eq!(recovered_status["readiness"]["quota"], "ready");
    assert_eq!(recovered_status["readiness"]["tracking"], "ready");
    assert_eq!(recovered_status["readiness"]["budget"], "verified");
    assert_eq!(recovered_status["readiness"]["dispatch"], "ready");
    assert_eq!(recovered_status["runnable"], true);
    assert_eq!(recovered_status["five_hour"]["reset_validity"], "future");
    assert!(
        recovered_status["issues"]
            .as_array()
            .is_some_and(|issues| { !issues.iter().any(|issue| issue["code"] == "missing_reset") })
    );
    assert_eq!(recovered_status["model"], "claude-sonnet-4-5");
    assert_eq!(recovered_status["model_guard_validity"], "match");
    assert_eq!(
        recovered_status["spend_period_baseline"]["verification"],
        "verified"
    );
    assert_eq!(
        recovered_status["seven_day"]["used_percentage_basis_points"],
        9_200
    );
    assert_used_field_identity(
        recovered_status,
        "five_hour",
        2_000,
        NOW + 900,
        NOW + 901,
        &binding.account_id,
    );
    assert_used_field_identity(
        recovered_status,
        "seven_day",
        9_200,
        NOW + 900,
        NOW + 901,
        &binding.account_id,
    );
    let recovered_boundary_state = &recovered_boundary.state;
    let recovered_boundary_account =
        persisted_account(recovered_boundary_state, &binding.account_id);
    assert_session_complete_projection(
        recovered_boundary_account,
        2_000,
        NOW + 4_000,
        NOW + 900,
        NOW + 901,
    );
    assert_long_range_identity(
        recovered_boundary_state,
        &binding.account_id,
        NOW + 900,
        NOW + 901,
    );
    assert!(
        recovered_boundary_state["monitors"][monitor_id.as_str()]["reset_barriers"][0].is_null()
    );
    assert!(
        recovered_boundary_state["accounts"][binding.account_id.as_str()]["reset_barriers"][0]
            .is_null()
    );
    recovered_status
}

fn assert_recovered_cli_and_authority(
    scenario: &mut ResetlessCliScenario<'_>,
    recovered_status: &serde_json::Value,
) {
    let start = scenario.start;
    let harness = start.harness;
    let binding = start.binding;
    let monitor_id = start.monitor_id;
    let observer_monitor_id = start.observer_monitor_id;
    let broker = start.broker;
    let binary = start.binary;
    let executor = start.executor;
    let watcher = &mut *scenario.watcher;
    let event_cursor = &mut *scenario.event_cursor;
    let consumed_event_ledger = &mut *scenario.consumed_event_ledger;

    let recovered_cli = run_cli(
        status_command(broker, binary, monitor_id),
        Duration::from_secs(5),
    );
    assert_eq!(recovered_cli.status.code(), Some(0));
    let recovered_cli_reply = json_output(&recovered_cli);
    let recovered_cli_status = &recovered_cli_reply["status"];
    assert_eq!(recovered_cli_status, recovered_status);
    let (_, events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "dispatch guard recovery CLI status",
    );
    consumed_event_ledger.extend(events);
    let recovered_observer_output = run_cli(
        status_command(broker, binary, observer_monitor_id),
        Duration::from_secs(5),
    );
    assert_eq!(recovered_observer_output.status.code(), Some(2));
    let recovered_observer_reply = json_output(&recovered_observer_output);
    assert_eq!(recovered_observer_reply["result"], "status");
    assert_observer_not_dispatchable(
        &recovered_observer_reply["status"],
        observer_monitor_id,
        2_000,
        NOW + 4_000,
        true,
    );
    let (recovered_observer_boundary, events) = drain_watch_boundary(
        harness,
        watcher,
        monitor_id,
        event_cursor,
        "observer recovery CLI status",
    );
    consumed_event_ledger.extend(events);
    assert_calls(executor, &harness.capability, 4);
    let recovered_state = &recovered_observer_boundary.state;
    assert_monitor_authority_config(
        recovered_state,
        monitor_id,
        binding,
        "dispatch_guard",
        false,
    );
    assert_monitor_authority_config(
        recovered_state,
        observer_monitor_id,
        binding,
        "observe_only",
        true,
    );
    let recovered_account = persisted_account(recovered_state, &binding.account_id);
    assert_session_complete_projection(recovered_account, 2_000, NOW + 4_000, NOW + 900, NOW + 901);
    assert_long_range_identity(recovered_state, &binding.account_id, NOW + 900, NOW + 901);
    let recovered_identity =
        session_durable_identity(recovered_state, &binding.account_id, monitor_id);
    assert_eq!(recovered_identity.latest_reset_epoch, NOW + 4_000);
    assert!(recovered_state["monitors"][monitor_id.as_str()]["reset_barriers"][0].is_null());
    assert!(
        recovered_state["accounts"][binding.account_id.as_str()]["reset_barriers"][0].is_null()
    );
    assert!(
        !broker
            .data_dir()
            .join(crate::BROKER_DIR)
            .join(crate::BROKER_RUN_DIR)
            .join(crate::BROKER_LEADER)
            .exists(),
        "CLI status and watch attach to the active socket without starting a broker"
    );
}

fn finish_long_watch_and_check_ledger(scenario: &mut ResetlessCliScenario<'_>) {
    let start = scenario.start;
    let harness = start.harness;
    let monitor_id = start.monitor_id;
    let watcher = &mut *scenario.watcher;
    let event_cursor = &mut *scenario.event_cursor;
    let consumed_event_ledger = &mut *scenario.consumed_event_ledger;

    let watch_output = watcher.finish(Duration::from_secs(LONG_WATCH_FINISH_TIMEOUT_SECS));
    assert_eq!(watch_output.status.code(), Some(0));
    assert!(watch_output.stderr.is_empty());
    let final_watch_boundary = capture_watch_boundary(harness, monitor_id, *event_cursor);
    assert_eq!(
        final_watch_boundary.target_cursor, *event_cursor,
        "long watch exited with an unconsumed durable suffix: cursor={event_cursor}, target={}, events={:?}",
        final_watch_boundary.target_cursor, final_watch_boundary.events
    );
    assert_watch_boundary_unchanged(
        harness,
        monitor_id,
        *event_cursor,
        &final_watch_boundary,
        "long watch exit",
    );
    assert_eq!(
        consumed_event_ledger
            .last()
            .and_then(|event| event["sequence"].as_u64()),
        Some(*event_cursor),
        "retained watch event ledger ends at the actually consumed cursor"
    );
    for pair in consumed_event_ledger.windows(2) {
        let previous = pair[0]["sequence"]
            .as_u64()
            .expect("consumed event ledger retains the previous sequence");
        let expected = previous
            .checked_add(1)
            .expect("consumed event ledger sequence does not overflow");
        assert_eq!(
            pair[1]["sequence"].as_u64(),
            Some(expected),
            "consumed event ledger contains a gap or reorder: pair={pair:?}"
        );
    }
}

fn stop_observer_and_assert_no_fifth_probe(scenario: &mut ResetlessCliScenario<'_>) {
    let start = scenario.start;
    let harness = start.harness;
    let monitor_id = start.monitor_id;
    let observer_monitor_id = start.observer_monitor_id;
    let binding = start.binding;
    let ticker = start.ticker;
    let executor = start.executor;
    let probe_starts = start.probe_starts;

    let stopped_observer = harness
        .store
        .operate(
            MonitorOperation::Stop {
                monitor_id: observer_monitor_id.clone(),
            },
            NOW + 902,
        )
        .expect("stop only the collection observer after the dispatch proof");
    match stopped_observer {
        MonitorReply::Stopped { status } => {
            assert_eq!(status.monitor_id, *observer_monitor_id);
            assert_eq!(status.purpose, MonitorPurpose::ObserveOnly);
        }
        other => panic!("expected observer stop reply, got {other:?}"),
    }
    assert!(harness.store.collection_accounts().is_empty());
    let stopped_state = monitor_state(harness);
    assert_monitor_authority_config(&stopped_state, monitor_id, binding, "dispatch_guard", false);
    assert_monitor_authority_config(
        &stopped_state,
        observer_monitor_id,
        binding,
        "observe_only",
        true,
    );
    assert!(stopped_state["monitors"][monitor_id.as_str()]["stopped_at_epoch"].is_null());
    assert_eq!(
        stopped_state["monitors"][observer_monitor_id.as_str()]["stopped_at_epoch"],
        NOW + 902
    );
    ticker.tick(harness, NOW + 1_200);
    assert_calls(executor, &harness.capability, 4);
    assert!(matches!(probe_starts.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
#[ignore = "requires JACKIN_TEST_CLI_BIN pointing to a verified jackin executable"]
fn actual_cli_tracks_resetless_collection_and_exact_paired_recovery() {
    // The workspace Nextest profile has a finite 60-second slow period and terminates after
    // three periods. The bounded stage waits fit under the 90-second real watch lifetime.
    assert_real_clock_precedes_fixture();
    let binary = cli_binary();
    let (scripted_executor, probe_starts, probe_releases) = ScriptedExecutor::new([
        provider_view_with_windows(
            NOW,
            9_500,
            Some(SESSION_RESET_EPOCH),
            9_200,
            LONG_RESET_EPOCH,
        ),
        provider_view_with_windows(NOW + 300, 0, None, 9_200, LONG_RESET_EPOCH),
        provider_view_with_windows(
            NOW + 600,
            2_000,
            Some(SESSION_RESET_EPOCH),
            9_200,
            LONG_RESET_EPOCH,
        ),
        provider_view_with_windows(NOW + 900, 2_000, Some(NOW + 4_000), 9_200, LONG_RESET_EPOCH),
    ]);
    let executor = Arc::new(scripted_executor);
    let executor_trait: Arc<dyn UsageProviderExecutor> = Arc::<ScriptedExecutor>::clone(&executor);
    let harness = CollectorHarness::new(executor_trait);
    let (binding, monitor_id) = start_dispatch_monitor(&harness);
    assert!(harness.store.collection_accounts().is_empty());
    assert_calls(&executor, &harness.capability, 0);
    let observer_monitor_id = start_observer_monitor(&harness, &binding);
    assert_ne!(observer_monitor_id, monitor_id);
    assert_calls(&executor, &harness.capability, 0);
    let authority_state = monitor_state(&harness);
    assert_monitor_authority_config(
        &authority_state,
        &monitor_id,
        &binding,
        "dispatch_guard",
        false,
    );
    assert_monitor_authority_config(
        &authority_state,
        &observer_monitor_id,
        &binding,
        "observe_only",
        true,
    );
    let broker = LocalBroker::start(&harness);
    let ticker = FakeTicker::start(&harness);

    let start = ResetlessCliStart {
        binary: &binary,
        executor: &executor,
        probe_starts: &probe_starts,
        probe_releases: &probe_releases,
        harness: &harness,
        binding: &binding,
        monitor_id: &monitor_id,
        observer_monitor_id: &observer_monitor_id,
        broker: &broker,
        ticker: &ticker,
    };
    let (initial_reply, initial_state, initial_session_identity) = start_initial_generation(&start);
    let initial_status = &initial_reply["status"];

    let initial_cursor = initial_state["monitors"][monitor_id.as_str()]["next_event_sequence"]
        .as_u64()
        .expect("pre-launch durable state includes its watch target");
    let initial_boundary =
        DurableWatchBoundary::from_state(initial_state.clone(), &monitor_id, initial_cursor);
    let watch_start_accepts = broker.accepted_count();
    let mut watcher = WatchProcess::spawn(watch_command(
        &broker,
        &binary,
        &monitor_id,
        LONG_WATCH_TIMEOUT_SECS,
    ));
    wait_for_accept_count(&broker, watch_start_accepts + 2);
    let initial_events = watcher.initial_events(&monitor_id, initial_cursor, initial_status);
    assert_eq!(
        initial_events.len(),
        1,
        "Watch(after_sequence=0) anchors on only the latest event before live suffix consumption"
    );
    assert!(
        !initial_events.is_empty(),
        "watch emits the current monitor event stream"
    );
    let durable_initial_event = initial_state["monitors"][monitor_id.as_str()]["events"]
        .as_array()
        .and_then(|events| events.last())
        .expect("pre-launch state retains its latest monitor event");
    assert_eq!(
        initial_events.last(),
        Some(durable_initial_event),
        "initial watch anchor must equal the exact pre-launch durable event"
    );
    let mut event_cursor = initial_cursor;
    assert_watch_boundary_unchanged(
        &harness,
        &monitor_id,
        event_cursor,
        &initial_boundary,
        "initial watch anchor",
    );
    let mut consumed_event_ledger = vec![
        initial_events
            .last()
            .expect("initial watch returned the current event")
            .clone(),
    ];

    {
        let mut scenario = ResetlessCliScenario {
            start: &start,
            watcher: &mut watcher,
            event_cursor: &mut event_cursor,
            consumed_event_ledger: &mut consumed_event_ledger,
            initial_reply: &initial_reply,
            initial_state: &initial_state,
            initial_session_identity: &initial_session_identity,
        };
        let orphan_sequence = publish_generation_two(&mut scenario);
        assert_sample_302_and_cli_status(&mut scenario, orphan_sequence);
        assert_grace_checkpoint(&mut scenario, orphan_sequence);
        let (old_reset_boundary, old_reset_events) = publish_old_reset(&mut scenario);
        let old_reset_identity =
            assert_old_reset(&mut scenario, &old_reset_boundary, &old_reset_events);
        refresh_recovery_model(&mut scenario, &old_reset_identity);
        record_recovery_spend(&mut scenario);
        assert_recovery_spend_refresh(&mut scenario, &old_reset_identity);
        let (recovered_boundary, recovered_events) = publish_paired_recovery(&mut scenario);
        let recovered_status =
            assert_paired_recovery(&mut scenario, &recovered_boundary, &recovered_events);
        assert_recovered_cli_and_authority(&mut scenario, recovered_status);
        finish_long_watch_and_check_ledger(&mut scenario);
        stop_observer_and_assert_no_fifth_probe(&mut scenario);
    }

    drop(ticker);
    drop(broker);
}
