// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn healthy_accounts_publish_while_one_account_stalls() {
    let temp = tempfile::tempdir().unwrap();
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(StallOneExecutor {
        slow: second_capability(),
        release: Mutex::new(release_rx),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();

    let before = client.current_projection().unwrap();
    let opened = client.subscribe_all([capability(), second_capability()]);
    assert!(opened.iter().all(|(_, result)| result.is_ok()));
    let fast = opened
        .iter()
        .find(|(item, _)| *item == capability())
        .unwrap()
        .1
        .as_ref()
        .unwrap()
        .clone();
    client
        .join(capability(), fast.generation, Duration::from_secs(5))
        .unwrap();

    // The healthy account is published with data while the stalled account
    // keeps its refreshing state; the catalog revision never changes.
    let partial = client.current_projection().unwrap();
    partial.validate().unwrap();
    assert_eq!(partial.discovery_revision, before.discovery_revision);
    assert!(partial.broker_generation > before.broker_generation);
    assert_eq!(
        partial.refresh_state,
        UsageProjectionRefreshStateV1::Refreshing
    );
    let providers = partial
        .providers
        .iter()
        .map(|provider| provider.provider_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(providers, vec!["claude", "codex"]);
    let fast_account = partial.providers[0]
        .accounts
        .iter()
        .find(|account| account.canonical_account_id == "abc123")
        .unwrap();
    assert_eq!(fast_account.freshness.phase, UsageFreshnessPhaseV1::Current);
    assert!(!fast_account.windows.is_empty());
    let slow_account = partial.providers[1]
        .accounts
        .iter()
        .find(|account| account.canonical_account_id == "def456")
        .unwrap();
    assert_eq!(
        slow_account.freshness.phase,
        UsageFreshnessPhaseV1::Refreshing
    );
    assert!(slow_account.windows.is_empty());

    release_tx.send(()).unwrap();
    let slow = opened
        .iter()
        .find(|(item, _)| *item == second_capability())
        .unwrap()
        .1
        .as_ref()
        .unwrap()
        .clone();
    client
        .join(second_capability(), slow.generation, Duration::from_secs(5))
        .unwrap();
    let settled = client.current_projection().unwrap();
    settled.validate().unwrap();
    assert_eq!(settled.discovery_revision, before.discovery_revision);
    assert!(settled.broker_generation > partial.broker_generation);
    assert_eq!(settled.refresh_state, UsageProjectionRefreshStateV1::Idle);
    assert!(
        settled
            .providers
            .iter()
            .flat_map(|provider| &provider.accounts)
            .all(|account| account.freshness.phase == UsageFreshnessPhaseV1::Current)
    );
}

#[test]
fn projection_refresh_runs_due_checks_and_join_settles() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete_executor;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        broker_executor,
    )
    .unwrap();
    client.subscribe(capability()).unwrap();
    client
        .join(capability(), 1, Duration::from_secs(5))
        .unwrap();
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    // A non-forced projection refresh reuses the still-fresh observation.
    let reused = client.request_refresh(None, false).unwrap();
    reused.validate().unwrap();
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert!(
        reused
            .providers
            .iter()
            .flat_map(|provider| &provider.accounts)
            .any(|account| account.canonical_account_id == "abc123")
    );

    // A forced projection refresh still respects Claude's minimum attempt
    // interval; the join observes the unchanged projection settle without
    // cancelling broker ownership.
    //
    // Join returns a superseding publication immediately by design, and
    // every intermediate publish mints a fresh publication id, so a single
    // join can observe a still-Refreshing snapshot under load. Chase the
    // chain until Idle or the deadline, like any correct caller must.
    let refreshing = client.request_refresh(None, true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut target = refreshing.projection_id.clone();
    let settled = loop {
        let observed = client
            .join_publication(target.clone(), Duration::from_secs(5))
            .unwrap();
        if observed.refresh_state == UsageProjectionRefreshStateV1::Idle
            || Instant::now() >= deadline
        {
            break observed;
        }
        target = observed.projection_id.clone();
    };
    assert_eq!(settled.refresh_state, UsageProjectionRefreshStateV1::Idle);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    // A superseded or unknown publication id returns the latest publication.
    let latest = client
        .join_publication("usage-broker:unknown".to_owned(), Duration::from_secs(5))
        .unwrap();
    assert_eq!(latest.projection_id, settled.projection_id);
}

#[test]
fn join_publication_timeout_leaves_broker_ownership_intact() {
    let temp = tempfile::tempdir().unwrap();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(HeldExecutor {
        started: started_tx,
        release: Mutex::new(release_rx),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();
    client.subscribe(capability()).unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // Wait for a quiesced refreshing publication: once the id is stable
    // across a ticker interval, no publish can interleave with the join below
    // until the probe is released.
    let refreshing = loop {
        let first = client.current_projection().unwrap();
        thread::park_timeout(Duration::from_millis(250));
        let second = client.current_projection().unwrap();
        if first.projection_id == second.projection_id
            && second.refresh_state == UsageProjectionRefreshStateV1::Refreshing
        {
            break second;
        }
    };

    let error = client
        .join_publication(refreshing.projection_id.clone(), Duration::from_millis(50))
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::WaitTimeout);

    // The timed-out join cancelled nothing: releasing the probe still settles
    // the same account generation into a newer publication.
    release_tx.send(()).unwrap();
    let settled = client
        .join_publication(refreshing.projection_id, Duration::from_secs(5))
        .unwrap();
    assert_eq!(settled.refresh_state, UsageProjectionRefreshStateV1::Idle);
    assert!(settled.broker_generation > refreshing.broker_generation);
}

#[test]
fn probe_budget_returns_fast_and_expires_without_waiting() {
    let fast = probe::run_probe_with_budget(Duration::from_secs(5), || 7_u32).unwrap();
    assert_eq!(fast, 7);

    let started = Instant::now();
    let expired = probe::run_probe_with_budget(Duration::from_millis(20), || {
        thread::park_timeout(Duration::from_secs(30));
        7_u32
    });
    assert_eq!(expired, Err(probe::ProbeBudgetExpired));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "budget expiry waited for the probe"
    );

    let timeout = probe::probe_timeout_outcome();
    let ProviderProbeOutcome::Failure {
        kind,
        message,
        retry_at_epoch,
    } = timeout
    else {
        panic!("budget expiry must report failure, never empty success");
    };
    assert_eq!(kind, UsageCoordinationErrorKind::ProviderTimeout);
    assert!(!message.is_empty());
    assert_eq!(retry_at_epoch, None);
}

#[test]
fn probe_budget_propagates_worker_panic_to_coordinator_classification() {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        probe::run_probe_with_budget(Duration::from_secs(5), || {
            panic!("adapter panic must reach the coordinator")
        })
    }));
    assert!(
        outcome.is_err(),
        "worker panic must propagate to the caller"
    );
}
