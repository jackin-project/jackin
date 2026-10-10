// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_usage_coordinator::{
    AccountStateStore, FileAccountStateStore, UsageCoordinator, UsageCoordinatorConfig,
};
use jackin_usage_provider_core::{ProviderError, get_json_bearer};

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

fn fake_retry_after_server(
    first_retry_after: &str,
) -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
    use std::io::{Read as _, Write as _};

    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let first_retry_after = first_retry_after.to_owned();
    let server_requests = Arc::new(AtomicUsize::new(0));
    let observed_requests = Arc::clone(&server_requests);
    let server = thread::spawn(move || {
        for index in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let _read = stream.read(&mut request).unwrap();
            observed_requests.fetch_add(1, Ordering::SeqCst);
            if index == 0 {
                write!(
                    stream,
                    "HTTP/1.1 429 Too Many Requests\r\nRetry-After: {first_retry_after}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
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

    (format!("http://{address}/usage"), server_requests, server)
}

fn verify_retry_after_survives_restart(retry_after_header: &str, minimum_retry_at: i64) -> i64 {
    let (url, server_requests, server) = fake_retry_after_server(retry_after_header);
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(FileAccountStateStore::at(temp.path().join("accounts")));
    let account = UsageAccountCapability {
        account_id: "retry-after-account".to_owned(),
        surface_id: "claude".to_owned(),
    };
    let started_at_epoch = chrono::Utc::now().timestamp();
    let clock = Arc::new(PairedTestClock::at(started_at_epoch));
    let executor = Arc::new(RetryAfterHttpExecutor {
        url: url.clone(),
        calls: AtomicUsize::new(0),
    });
    let executor_clone = Arc::<RetryAfterHttpExecutor>::clone(&executor);
    let executor_trait: Arc<dyn UsageProviderExecutor> = executor_clone;
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce the paired test clock into the coordinator clock port"
    )]
    let clock_trait: Arc<dyn MonotonicClock> = clock.clone();
    let coordinator = UsageCoordinator::new_with_clock(
        executor_trait,
        Arc::<FileAccountStateStore>::clone(&store),
        UsageCoordinatorConfig::default(),
        clock_trait,
    );

    let first = coordinator
        .request_refresh(&account, 0, true, started_at_epoch)
        .unwrap();
    let first_completion_epoch = started_at_epoch.saturating_add(1);
    clock.advance_to_epoch(first_completion_epoch);
    let failed = coordinator
        .join_generation(
            &account,
            first.generation,
            Duration::from_secs(5),
            first_completion_epoch,
        )
        .unwrap();
    assert_eq!(failed.phase, UsageRefreshPhase::Failed);
    let retry_at = failed.retry_at_epoch.expect("provider retry deadline");
    assert!(retry_at >= minimum_retry_at);
    let durable = store
        .load(&account, started_at_epoch.saturating_add(1))
        .unwrap()
        .unwrap();
    assert_eq!(durable.rate_limit_deadline_epoch, Some(retry_at));
    assert_eq!(durable.retry_deadline_epoch, Some(retry_at));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(server_requests.load(Ordering::SeqCst), 1);
    drop(coordinator);

    let restarted_executor = Arc::new(RetryAfterHttpExecutor {
        url,
        calls: AtomicUsize::new(0),
    });
    let restarted_executor_clone = Arc::<RetryAfterHttpExecutor>::clone(&restarted_executor);
    let restarted_executor_trait: Arc<dyn UsageProviderExecutor> = restarted_executor_clone;
    let restarted_at_epoch = clock.wall_epoch();
    let restarted_clock = Arc::new(PairedTestClock::at(restarted_at_epoch));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce the paired test clock into the coordinator clock port"
    )]
    let restarted_clock_trait: Arc<dyn MonotonicClock> = restarted_clock.clone();
    let restarted = UsageCoordinator::new_with_clock(
        restarted_executor_trait,
        store,
        UsageCoordinatorConfig::default(),
        restarted_clock_trait,
    );
    let restored = restarted.current(&account, restarted_at_epoch).unwrap();
    assert_eq!(restored.generation, first.generation);
    assert_eq!(restored.retry_at_epoch, Some(retry_at));
    let before_reload_floor = restarted
        .request_refresh(&account, restored.generation, true, restarted_at_epoch)
        .unwrap();
    assert_eq!(before_reload_floor.generation, first.generation);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);

    restarted_clock.advance_to_epoch(retry_at.saturating_sub(1));
    let forced_early = restarted
        .request_refresh(
            &account,
            restored.generation,
            true,
            retry_at.saturating_sub(1),
        )
        .unwrap();
    assert_eq!(forced_early.generation, first.generation);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 0);
    assert_eq!(server_requests.load(Ordering::SeqCst), 1);

    restarted_clock.advance_to_epoch(retry_at);
    let allowed = restarted
        .request_refresh(&account, restored.generation, true, retry_at)
        .unwrap();
    assert_eq!(allowed.generation, first.generation + 1);
    restarted_clock.advance_to_epoch(retry_at.saturating_add(1));
    let completed = restarted
        .join_generation(
            &account,
            allowed.generation,
            Duration::from_secs(5),
            retry_at + 1,
        )
        .unwrap();
    assert_eq!(completed.phase, UsageRefreshPhase::Completed);
    assert_eq!(restarted_executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(server_requests.load(Ordering::SeqCst), 2);
    server.join().unwrap();
    retry_at
}

#[test]
fn numeric_and_http_date_retry_after_survive_restart_without_early_requests() {
    let numeric_minimum = chrono::Utc::now().timestamp().saturating_add(600);
    let numeric_retry_at = verify_retry_after_survives_restart("600", numeric_minimum);
    assert!(numeric_retry_at >= numeric_minimum);

    let reset_at = chrono::Utc::now().timestamp().saturating_add(3_600);
    let retry_after = chrono::DateTime::<chrono::Utc>::from_timestamp(reset_at, 0)
        .unwrap()
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    let date_retry_at = verify_retry_after_survives_restart(&retry_after, reset_at);
    assert_eq!(date_retry_at, reset_at);
}
