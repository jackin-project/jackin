// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::Money;
use jackin_protocol::usage_monitor::{
    MonitorAccountBindingInput, MonitorConfig, MonitorIssueCode, MonitorOperation, MonitorPolicy,
    MonitorPolicyApprovalInput, MonitorProvider, MonitorPurpose, MonitorReply, MonitorScope,
    MonitorStatus, SpendRecordInput, SpendRecordSource, StatuslineObservation,
    StatuslineQuotaWindow, StatuslineRateLimits, USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
};
use jackin_usage_coordinator::{FileAccountStateStore, UsageCoordinator, UsageCoordinatorConfig};
use jackin_usage_provider_core::{ProviderError, get_json_bearer};
use std::io::{ErrorKind, Read as _};
use std::net::{Ipv4Addr, TcpListener, TcpStream};

struct RetryAfterHttpExecutor {
    url: String,
    calls: AtomicUsize,
}

const FIXTURE_MAX_REQUEST_HEADER_BYTES: usize = 16 * 1024;

struct RetryAfterServer {
    url: String,
    requests: Arc<AtomicUsize>,
    server: thread::JoinHandle<std::io::Result<()>>,
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

fn fake_retry_after_server() -> RetryAfterServer {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let observed_requests = Arc::clone(&requests);
    let server = thread::spawn(move || -> std::io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        for index in 0..2 {
            loop {
                let mut stream = accept_fixture_stream(&listener, deadline)?;
                let request_deadline = (Instant::now() + Duration::from_secs(4)).min(deadline);
                match read_fixture_request_headers(&mut stream, request_deadline) {
                    Ok(_request) => {
                        observed_requests.fetch_add(1, Ordering::SeqCst);
                        write_fixture_response(&mut stream, index)?;
                        break;
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            ErrorKind::UnexpectedEof | ErrorKind::TimedOut | ErrorKind::WouldBlock
                        ) =>
                    {
                        // Ignore an incomplete connection and accept a replacement, without
                        // counting it as a provider request or consuming either response.
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(())
    });
    RetryAfterServer {
        url: format!("http://{address}/usage"),
        requests,
        server,
    }
}

fn write_fixture_response(stream: &mut TcpStream, request_index: usize) -> std::io::Result<()> {
    let response = match request_index {
        0 => b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 600\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".as_slice(),
        1 => b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}".as_slice(),
        _ => {
            return Err(std::io::Error::new(
                ErrorKind::InvalidInput,
                "unexpected request index in Retry-After fixture",
            ));
        }
    };
    stream.write_all(response)
}

fn accept_fixture_stream(listener: &TcpListener, deadline: Instant) -> std::io::Result<TcpStream> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                ErrorKind::TimedOut,
                "deadline elapsed while waiting for a provider request connection",
            ));
        }
        match listener.accept() {
            Ok((stream, _)) => return Ok(stream),
            Err(error) if error.kind() == ErrorKind::Interrupted => {
                thread::park_timeout(Duration::from_millis(1).min(remaining));
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::park_timeout(Duration::from_millis(5).min(remaining));
            }
            Err(error) => return Err(error),
        }
    }
}

fn read_fixture_request_headers(
    stream: &mut TcpStream,
    deadline: Instant,
) -> std::io::Result<Vec<u8>> {
    let mut request = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining < Duration::from_millis(1) {
            return Err(std::io::Error::new(
                ErrorKind::TimedOut,
                "deadline elapsed before provider request headers were complete",
            ));
        }
        let read_timeout = Duration::from_millis(100).min(remaining);
        stream.set_read_timeout(Some(read_timeout))?;
        match stream.read(&mut chunk) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "provider request ended before its headers were complete",
                ));
            }
            Ok(read) => {
                request.extend_from_slice(&chunk[..read]);
                if request.len() > FIXTURE_MAX_REQUEST_HEADER_BYTES {
                    return Err(std::io::Error::new(
                        ErrorKind::InvalidData,
                        "provider request headers exceeded the fixture limit",
                    ));
                }
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    return Ok(request);
                }
                if Instant::now() >= deadline {
                    return Err(std::io::Error::new(
                        ErrorKind::TimedOut,
                        "deadline elapsed before provider request headers were complete",
                    ));
                }
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {
                thread::park_timeout(
                    Duration::from_millis(1)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if Instant::now() >= deadline {
                    return Err(std::io::Error::new(
                        ErrorKind::TimedOut,
                        "deadline elapsed while waiting for provider request headers",
                    ));
                }
                thread::park_timeout(
                    Duration::from_millis(5)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            Err(error) => return Err(error),
        }
    }
}

fn local_fixture_tcp_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let writer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (reader, _) = listener.accept().unwrap();
    (reader, writer)
}

#[test]
fn fixture_reader_handles_fragmented_headers_and_rejects_incomplete_or_oversized_input() {
    let (mut reader, mut writer) = local_fixture_tcp_pair();
    let writer_thread = thread::spawn(move || -> std::io::Result<()> {
        writer.write_all(b"GET /usage HTTP/1.1\r\nHost: ")?;
        thread::park_timeout(Duration::from_millis(10));
        writer.write_all(b"localhost\r\n\r\n")
    });
    let deadline = Instant::now() + Duration::from_secs(1);
    let headers = read_fixture_request_headers(&mut reader, deadline)
        .expect("read fragmented complete request headers");
    writer_thread
        .join()
        .unwrap()
        .expect("write fragmented headers");
    assert_eq!(headers, b"GET /usage HTTP/1.1\r\nHost: localhost\r\n\r\n");

    let (mut reader, mut writer) = local_fixture_tcp_pair();
    writer.write_all(b"GET /usage HTTP/1.1\r\nHost:").unwrap();
    drop(writer);
    let error = read_fixture_request_headers(&mut reader, Instant::now() + Duration::from_secs(1))
        .expect_err("reject request that closes before completing headers");
    assert_eq!(error.kind(), ErrorKind::UnexpectedEof);

    let (mut reader, mut writer) = local_fixture_tcp_pair();
    writer
        .write_all(&vec![b'x'; FIXTURE_MAX_REQUEST_HEADER_BYTES + 1])
        .unwrap();
    let error = read_fixture_request_headers(&mut reader, Instant::now() + Duration::from_secs(1))
        .expect_err("reject oversized request headers");
    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

fn monitor_observation(reset_at_epoch: i64) -> StatuslineObservation {
    let quota = || {
        Some(StatuslineQuotaWindow {
            used_percentage_basis_points: Some(8_000),
            reset_at_epoch: Some(reset_at_epoch),
        })
    };
    StatuslineObservation {
        schema_version: USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
        session_id: "session-reset".to_owned(),
        model: None,
        claude_code_version: Some("2.1.80".to_owned()),
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

fn start_reset_barrier_monitor(
    data_dir: &std::path::Path,
    account_id: &str,
    reset_at_epoch: i64,
    now_epoch: i64,
) -> (MonitorStore, String) {
    let monitor = MonitorStore::open(data_dir).expect("open monitor store");
    let binding = match monitor
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: account_id.to_owned(),
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                    provider_account_id: None,
                    experimental_collector_approved: false,
                },
            },
            now_epoch,
        )
        .expect("bind isolated monitor account")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    };
    monitor
        .operate(
            MonitorOperation::Ingest {
                scope: MonitorScope::BoundAccount {
                    binding_id: binding.binding_id.clone(),
                    binding_revision: binding.revision,
                    session_id: None,
                },
                observation: monitor_observation(reset_at_epoch),
            },
            now_epoch,
        )
        .expect("ingest statusline evidence");
    monitor
        .operate(
            MonitorOperation::RecordSpend {
                record: SpendRecordInput {
                    account_id: account_id.to_owned(),
                    billing_period_start_epoch: now_epoch - 100,
                    billing_period_end_epoch: now_epoch + 100_000,
                    amount: Money::new(0, "SGD", 2),
                    evidence_at_epoch: Some(now_epoch),
                    verified: true,
                    source: SpendRecordSource::OperatorReceipt,
                },
            },
            now_epoch,
        )
        .expect("seed monitor spend baseline");
    let policy = match monitor
        .operate(
            MonitorOperation::ApprovePolicy {
                approval: MonitorPolicyApprovalInput {
                    binding_id: binding.binding_id.clone(),
                    binding_revision: binding.revision,
                    goal_id: "goal-reset".to_owned(),
                    new_policy: MonitorPolicy::StrictSgd,
                    budget: Some(Money::new(5_000, "SGD", 2)),
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                    acknowledge_no_sgd_cap: false,
                    expected_revision: None,
                },
            },
            now_epoch,
        )
        .expect("approve strict SGD policy in isolated fixture")
    {
        MonitorReply::PolicyApproved { policy } => policy,
        other => panic!("expected policy-approved reply, got {other:?}"),
    };
    let started = match monitor
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
                    goal_id: Some("goal-reset".to_owned()),
                    expected_model: None,
                    policy_revision: Some(policy.revision),
                    experimental_collector: false,
                },
                idempotency_key: "fixture-reset-barrier".to_owned(),
            },
            now_epoch,
        )
        .expect("start local monitor")
    {
        MonitorReply::Started { status } => *status,
        other => panic!("expected started reply, got {other:?}"),
    };
    (monitor, started.monitor_id)
}

fn assert_provider_request_counts(
    executor: &RetryAfterHttpExecutor,
    server_requests: &AtomicUsize,
    expected: usize,
) {
    assert_eq!(executor.calls.load(Ordering::SeqCst), expected);
    assert_eq!(server_requests.load(Ordering::SeqCst), expected);
}

fn assert_blocked_by_shared_retry_after(
    coordinator: &UsageCoordinator,
    account: &UsageAccountCapability,
    generation: u64,
    at_epoch: i64,
    retry_at_epoch: i64,
    executor: &RetryAfterHttpExecutor,
    server_requests: &AtomicUsize,
) {
    let blocked = coordinator
        .request_refresh(account, generation, true, at_epoch)
        .expect("forced refresh remains blocked by provider Retry-After");
    assert_eq!(blocked.generation, generation);
    assert_eq!(blocked.retry_at_epoch, Some(retry_at_epoch));
    assert_provider_request_counts(executor, server_requests, 1);
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

    let fixture = fake_retry_after_server();
    let executor = Arc::new(RetryAfterHttpExecutor {
        url: fixture.url,
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
    assert_eq!(fixture.requests.load(Ordering::SeqCst), 1);

    // A monitor clock tick is local reconciliation. Crossing the reset grace
    // deadline without a fresh statusline produces the safety barrier and does
    // not trigger another provider request.
    let (monitor, monitor_id) =
        start_reset_barrier_monitor(data_dir, &account.account_id, reset_at, now);

    monitor.tick(reset_at).expect("tick at reported reset");
    monitor
        .tick(reset_deadline)
        .expect("tick at reset grace deadline");
    let due = monitor_status(&monitor, &monitor_id, reset_deadline);
    assert!(
        due.issues
            .iter()
            .any(|issue| { issue.code == MonitorIssueCode::ResetDueUnverified })
    );
    assert_blocked_by_shared_retry_after(
        &coordinator,
        &account,
        first.generation,
        reset_deadline,
        retry_at,
        &executor,
        &fixture.requests,
    );

    // The attempt floor has elapsed by Retry-After minus one second, so this
    // assertion specifically proves the shared provider deadline still blocks
    // a forced refresh after the monitor's reset deadline.
    assert_blocked_by_shared_retry_after(
        &coordinator,
        &account,
        first.generation,
        retry_at - 1,
        retry_at,
        &executor,
        &fixture.requests,
    );

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
    assert_provider_request_counts(&executor, &fixture.requests, 2);
    fixture
        .server
        .join()
        .expect("join Retry-After fixture server")
        .expect("serve two complete provider requests");
}
