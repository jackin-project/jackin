// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::Money;
use jackin_protocol::usage_monitor::{
    MonitorConfig, MonitorIssueCode, MonitorOperation, MonitorProvider, MonitorReply,
    MonitorStatus, SpendRecordInput, SpendRecordSource, StatuslineObservation,
    StatuslineQuotaWindow, StatuslineRateLimits, USAGE_MONITOR_SCHEMA_VERSION,
};
use jackin_usage_coordinator::{FileAccountStateStore, UsageCoordinator, UsageCoordinatorConfig};
use jackin_usage_provider_core::{ProviderError, get_json_bearer};
use std::io::Read as _;
use std::net::{Ipv4Addr, TcpListener, TcpStream};

struct RetryAfterHttpExecutor {
    url: String,
    calls: AtomicUsize,
}

impl UsageProviderExecutor for RetryAfterHttpExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match get_json_bearer::<serde_json::Value>(
            jackin_telemetry::schema::enums::ProviderName::Anthropic,
            "claude.usage",
            "Claude usage",
            &self.url,
            "fixture-token",
            &[],
        ) {
            Ok(_) => ProviderProbeOutcome::success(quota_view()),
            Err(http_error) => {
                let error = ProviderError::from(http_error);
                let mut view = quota_view();
                view.status = UsageSnapshotStatus::Stale;
                view.last_error = Some(error.to_string());
                provider_probe_outcome_with_rate_limit(view, error.rate_limit())
            }
        }
    }
}

fn fake_retry_after_server() -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let observed_requests = Arc::clone(&requests);
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(8);
        for index in 0..2 {
            let Some(mut stream) = accept_fixture_stream(&listener, deadline) else {
                return;
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0_u8; 2048];
            let _read = stream.read(&mut request).unwrap();
            observed_requests.fetch_add(1, Ordering::SeqCst);
            if index == 0 {
                stream
                    .write_all(
                        b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 600\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                    )
                    .unwrap();
            } else {
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}",
                    )
                    .unwrap();
            }
        }
    });
    (format!("http://{address}/usage"), requests, server)
}

fn accept_fixture_stream(listener: &TcpListener, deadline: Instant) -> Option<TcpStream> {
    loop {
        let error = match listener.accept() {
            Ok((stream, _)) => return Some(stream),
            Err(error) => error,
        };
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::WouldBlock,
            "accept provider fixture request: {error}"
        );
        if Instant::now() >= deadline {
            return None;
        }
        thread::park_timeout(Duration::from_millis(5));
    }
}

fn monitor_observation(reset_at_epoch: i64) -> StatuslineObservation {
    let quota = || {
        Some(StatuslineQuotaWindow {
            used_percentage_basis_points: Some(8_000),
            reset_at_epoch: Some(reset_at_epoch),
        })
    };
    StatuslineObservation {
        schema_version: USAGE_MONITOR_SCHEMA_VERSION,
        session_id: "session-reset".to_owned(),
        model: None,
        rate_limits: StatuslineRateLimits {
            five_hour: quota(),
            seven_day: quota(),
        },
    }
}

fn monitor_status(store: &MonitorStore, monitor_id: &str, now_epoch: i64) -> MonitorStatus {
    match store
        .operate(
            MonitorOperation::Status {
                monitor_id: monitor_id.to_owned(),
            },
            now_epoch,
        )
        .expect("read monitor status")
    {
        MonitorReply::Status { status } => *status,
        other => panic!("expected status reply, got {other:?}"),
    }
}

#[test]
fn reset_barrier_does_not_bypass_shared_provider_retry_after() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path();
    let now = chrono::Utc::now().timestamp();
    let reset_at = now + 120;
    let reset_deadline = reset_at + 60;
    let account = UsageAccountCapability {
        account_id: "monitor-account".to_owned(),
        surface_id: "claude".to_owned(),
    };

    let (url, server_requests, server) = fake_retry_after_server();
    let executor = Arc::new(RetryAfterHttpExecutor {
        url,
        calls: AtomicUsize::new(0),
    });
    let executor_clone = Arc::<RetryAfterHttpExecutor>::clone(&executor);
    let executor_trait: Arc<dyn UsageProviderExecutor> = executor_clone;
    let account_store = Arc::new(FileAccountStateStore::at(data_dir.join("accounts")));
    let coordinator = UsageCoordinator::new(
        executor_trait,
        Arc::<FileAccountStateStore>::clone(&account_store),
        UsageCoordinatorConfig::default(),
    );

    let first = coordinator
        .request_refresh(&account, 0, true, now)
        .expect("start provider refresh");
    let failed = coordinator
        .join_generation(&account, first.generation, Duration::from_secs(5), now + 1)
        .expect("join rate-limited provider refresh");
    assert_eq!(failed.phase, UsageRefreshPhase::Failed);
    let retry_at = failed.retry_at_epoch.expect("Retry-After deadline");
    assert!(retry_at > reset_deadline);
    assert!(
        retry_at > now + 300,
        "exercise Retry-After beyond Claude's attempt floor"
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(server_requests.load(Ordering::SeqCst), 1);

    // A monitor clock tick is local reconciliation. Crossing the reset grace
    // deadline without a fresh statusline produces the safety barrier and does
    // not trigger another provider request.
    let monitor = MonitorStore::open(data_dir).expect("open monitor store");
    monitor
        .operate(
            MonitorOperation::Ingest {
                account_id: account.account_id.clone(),
                observation: monitor_observation(reset_at),
            },
            now,
        )
        .expect("ingest statusline evidence");
    monitor
        .operate(
            MonitorOperation::RecordSpend {
                record: SpendRecordInput {
                    account_id: account.account_id.clone(),
                    billing_period_start_epoch: now - 100,
                    billing_period_end_epoch: now + 100_000,
                    amount: Money::new(0, "SGD", 2),
                    evidence_at_epoch: Some(now),
                    verified: true,
                    source: SpendRecordSource::OperatorReceipt,
                },
            },
            now,
        )
        .expect("seed monitor spend baseline");
    let started = match monitor
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    account_id: account.account_id.clone(),
                    goal_id: "goal-reset".to_owned(),
                    session_id: None,
                    expected_model: None,
                    budget: Some(Money::new(5_000, "SGD", 2)),
                },
            },
            now,
        )
        .expect("start local monitor")
    {
        MonitorReply::Started { status } => *status,
        other => panic!("expected started reply, got {other:?}"),
    };

    monitor.tick(reset_at).expect("tick at reported reset");
    monitor
        .tick(reset_deadline)
        .expect("tick at reset grace deadline");
    let due = monitor_status(&monitor, &started.monitor_id, reset_deadline);
    assert!(
        due.issues
            .iter()
            .any(|issue| { issue.code == MonitorIssueCode::ResetDueUnverified })
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(server_requests.load(Ordering::SeqCst), 1);

    let forced_at_reset = coordinator
        .request_refresh(&account, first.generation, true, reset_deadline)
        .expect("blocked forced refresh at monitor reset deadline");
    assert_eq!(forced_at_reset.generation, first.generation);
    assert_eq!(forced_at_reset.retry_at_epoch, Some(retry_at));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(server_requests.load(Ordering::SeqCst), 1);

    // The attempt floor has elapsed by Retry-After minus one second, so this
    // assertion specifically proves the shared provider deadline still blocks
    // a forced refresh after the monitor's reset deadline.
    let forced_before_retry = coordinator
        .request_refresh(&account, first.generation, true, retry_at - 1)
        .expect("blocked early forced refresh is a settled response");
    assert_eq!(forced_before_retry.generation, first.generation);
    assert_eq!(forced_before_retry.retry_at_epoch, Some(retry_at));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(server_requests.load(Ordering::SeqCst), 1);

    let allowed = coordinator
        .request_refresh(&account, first.generation, true, retry_at)
        .expect("admit forced refresh at provider deadline");
    assert_eq!(allowed.generation, first.generation + 1);
    let completed = coordinator
        .join_generation(
            &account,
            allowed.generation,
            Duration::from_secs(5),
            retry_at + 1,
        )
        .expect("join request admitted at Retry-After deadline");
    assert_eq!(completed.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
    assert_eq!(server_requests.load(Ordering::SeqCst), 2);
    server.join().unwrap();
}
