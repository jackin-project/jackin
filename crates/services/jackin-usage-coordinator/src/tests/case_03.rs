// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn corrupt_revoked_account_is_quarantined_without_poisoning_rotation() {
    let temp = tempfile::tempdir().unwrap();
    let accounts = temp.path().join("accounts");
    std::fs::create_dir_all(&accounts).unwrap();
    let account = capability("account-a");
    let active = accounts.join("claude-account-a.json");
    std::fs::write(&active, b"corrupt revoked state").unwrap();
    std::fs::set_permissions(&active, std::fs::Permissions::from_mode(0o600)).unwrap();

    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(FileAccountStateStore::at(accounts.clone()));
    let coordinator = UsageCoordinator::with_catalog(
        executor,
        store,
        UsageCoordinatorConfig::default(),
        [catalog_entry(&account, "revision-a")],
    );

    coordinator.reconcile_catalog([], 1_001).unwrap();
    assert!(!active.exists());
    let quarantined = std::fs::read_dir(accounts)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".corrupt."))
        .collect::<Vec<_>>();
    assert_eq!(quarantined.len(), 1);
    assert_eq!(
        coordinator.current(&account, 1_001).unwrap_err().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
}

#[test]
fn coordinator_unknown_bootstrap_serializes_per_provider() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let bootstrap = capability("bootstrap-claude");
    coordinator
        .request_refresh(&bootstrap, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    let joined = coordinator
        .request_refresh(&bootstrap, 0, true, 1_000)
        .unwrap();
    assert_eq!(joined.generation, 1);
    executor.release(1);
    drop(join_ok(&coordinator, &bootstrap, 1, 1_001));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn coordinator_distinct_accounts_refresh_within_concurrency_bound() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let config = UsageCoordinatorConfig {
        max_concurrency: 2,
        ..UsageCoordinatorConfig::default()
    };
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        config,
    );
    let account_a = capability("account-a");
    let account_b = capability("account-b");
    coordinator
        .request_refresh(&account_a, 0, true, 1_000)
        .unwrap();
    coordinator
        .request_refresh(&account_b, 0, true, 1_000)
        .unwrap();
    executor.wait_started(2);
    assert_eq!(executor.max_active.load(Ordering::SeqCst), 2);
    executor.release(2);
    drop(join_ok(&coordinator, &account_a, 1, 1_001));
    drop(join_ok(&coordinator, &account_b, 1, 1_001));
}

#[test]
fn cadence_tiers_select_spec_intervals_with_bounded_jitter() {
    let account = capability("account-a");
    let cases = [
        (UsageActivity::DirectInteraction, false, 120..=150),
        (UsageActivity::Recent, false, 300..=375),
        (UsageActivity::Idle, false, 900..=1_125),
        (UsageActivity::LongIdle, false, 1_800..=2_250),
        (UsageActivity::DirectInteraction, true, 1_800..=2_250),
    ];
    for (activity, low_power, range) in cases {
        let first = cadence_deadline(activity, low_power, &account, 3, 10_000);
        assert!(
            range.contains(&(first - 10_000)),
            "{activity:?} low_power={low_power} out of range: {first}"
        );
        assert_eq!(
            first,
            cadence_deadline(activity, low_power, &account, 3, 10_000),
            "cadence deadline must be deterministic for joined callers"
        );
    }
}

#[test]
fn cadence_poll_due_fires_once_per_interval_and_honors_success_cooldown() {
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        cadence_quota_view(1_000),
    )));
    let coordinator = cadence_coordinator(Arc::clone(&executor), UsageCoordinatorConfig::default());
    let account = capability("account-a");
    coordinator
        .set_activity(&account, UsageActivity::DirectInteraction, false, 1_000)
        .unwrap();

    let started = coordinator.poll_due(1_000);
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].generation, 1);
    assert_eq!(
        join_ok(&coordinator, &account, 1, 1_001).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    let due = coordinator.next_due_epoch().unwrap();
    assert!((1_120..=1_150).contains(&due), "due={due}");
    assert!(coordinator.poll_due(due - 1).is_empty());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    let suppressed = coordinator.poll_due(due);
    assert_eq!(suppressed.len(), 1);
    assert_eq!(suppressed[0].generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let cooldown_due = coordinator.next_due_epoch().unwrap();
    assert!(
        (1_300..=1_310).contains(&cooldown_due),
        "shared success cooldown must win: {cooldown_due}"
    );

    let second = coordinator.poll_due(cooldown_due);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].generation, 2);
    assert_eq!(
        join_ok(&coordinator, &account, 2, cooldown_due + 1).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn cadence_wake_recalculates_without_missed_poll_burst() {
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
        cadence_quota_view(1_000),
    )));
    let coordinator = cadence_coordinator(Arc::clone(&executor), UsageCoordinatorConfig::default());
    let account = capability("account-a");
    coordinator
        .set_activity(&account, UsageActivity::DirectInteraction, false, 1_000)
        .unwrap();
    assert_eq!(coordinator.poll_due(1_000).len(), 1);
    drop(join_ok(&coordinator, &account, 1, 1_001));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    let wake = 1_000 + 36_000;
    assert_eq!(coordinator.note_wake(wake), 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let due = coordinator.next_due_epoch().unwrap();
    assert!((wake + 120..=wake + 150).contains(&due), "due={due}");
    assert!(coordinator.poll_due(wake).is_empty());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    let second = coordinator.poll_due(due);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].generation, 2);
    drop(join_ok(&coordinator, &account, 2, due + 1));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn cadence_shared_retry_after_wins_over_periodic_due() {
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::RateLimited,
        message: "provider rate limited".into(),
        retry_at_epoch: Some(5_000),
    }));
    let coordinator = cadence_coordinator(Arc::clone(&executor), UsageCoordinatorConfig::default());
    let account = capability("account-a");
    coordinator
        .set_activity(&account, UsageActivity::DirectInteraction, false, 1_000)
        .unwrap();
    assert_eq!(coordinator.poll_due(1_000).len(), 1);
    let failed = join_ok(&coordinator, &account, 1, 1_001);
    assert_eq!(failed.phase, UsageRefreshPhase::Failed);
    assert_eq!(failed.retry_at_epoch, Some(5_000));

    let due = coordinator.next_due_epoch().unwrap();
    assert!((1_120..=1_150).contains(&due), "due={due}");
    let suppressed = coordinator.poll_due(due);
    assert_eq!(suppressed.len(), 1);
    assert_eq!(suppressed[0].generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(coordinator.next_due_epoch(), Some(5_000));
    assert!(coordinator.poll_due(4_999).is_empty());

    executor.set_outcome(ProviderProbeOutcome::success(cadence_quota_view(5_000)));
    let second = coordinator.poll_due(5_000);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].generation, 2);
    assert_eq!(
        join_ok(&coordinator, &account, 2, 5_001).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn cadence_two_clients_single_flight_exactly_one_provider_call() {
    let executor = Arc::new(CadenceGateExecutor::new(ProviderProbeOutcome::success(
        cadence_quota_view(1_000),
    )));
    let coordinator = cadence_coordinator(Arc::clone(&executor), UsageCoordinatorConfig::default());
    let account = capability("account-a");
    coordinator
        .set_activity(&account, UsageActivity::DirectInteraction, false, 1_000)
        .unwrap();

    let winner = coordinator.poll_due(1_000);
    assert_eq!(winner.len(), 1);
    assert_eq!(winner[0].generation, 1);
    executor.wait_started();

    let manual_joiner = coordinator
        .request_refresh(&account, 1, true, 1_000)
        .unwrap();
    assert_eq!(manual_joiner.generation, 1);
    assert!(manual_joiner.phase.is_active());
    let repeated_manual = coordinator
        .request_refresh(&account, 1, true, 1_000)
        .unwrap();
    assert_eq!(repeated_manual.generation, 1);
    let due = coordinator.next_due_epoch().unwrap();
    let ambient_joiner = coordinator.poll_due(due);
    assert_eq!(ambient_joiner.len(), 1);
    assert_eq!(ambient_joiner[0].generation, 1);

    executor.release();
    let terminal = join_ok(&coordinator, &account, 1, 1_001);
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}
