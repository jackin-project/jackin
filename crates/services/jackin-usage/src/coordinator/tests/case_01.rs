// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn coordinator_winner_joiner_and_force_join_share_one_generation() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let store = Arc::new(MemoryStore::default());
    let coordinator = coordinator(
        Arc::clone(&executor),
        store,
        UsageCoordinatorConfig::default(),
    );
    let account = capability("account-a");

    let winner = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    let joiner = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    assert_eq!(winner.generation, 1);
    assert_eq!(joiner.generation, 1);
    assert!(joiner.phase.is_active());

    executor.release(1);
    let terminal = join_ok(&coordinator, &account, 1, 1_001);
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn coordinator_revisioned_capabilities_do_not_join_in_flight_probe() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let old_catalog_capability = capability("account-a:catalog-old");
    let current_catalog_capability = capability("account-a:catalog-current");

    let old = coordinator
        .request_refresh(&old_catalog_capability, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    let current = coordinator
        .request_refresh(&current_catalog_capability, 0, true, 1_000)
        .unwrap();
    executor.wait_started(2);

    assert_eq!(old.generation, 1);
    assert_eq!(current.generation, 1);
    executor.release(2);
    assert_eq!(
        join_ok(&coordinator, &old_catalog_capability, 1, 1_001).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(
        join_ok(&coordinator, &current_catalog_capability, 1, 1_001).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn coordinator_post_terminal_manual_refresh_starts_later_generation() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let account = capability("account-a");
    let first = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    executor.release(1);
    let first = join_ok(&coordinator, &account, first.generation, 1_001);

    let stale_click = coordinator
        .request_refresh(&account, 0, true, 1_002)
        .unwrap();
    assert_eq!(stale_click.generation, first.generation);
    let later_click = coordinator
        .request_refresh(&account, first.generation, true, 1_002)
        .unwrap();
    assert_eq!(later_click.generation, 2);
    executor.wait_started(2);
    executor.release(1);
    assert_eq!(
        join_ok(&coordinator, &account, 2, 1_003).phase,
        UsageRefreshPhase::Completed
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn coordinator_ambient_tick_honors_success_cooldown() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let account = capability("account-a");
    coordinator
        .request_refresh(&account, 0, false, 1_000)
        .unwrap();
    executor.wait_started(1);
    executor.release(1);
    let terminal = join_ok(&coordinator, &account, 1, 1_001);
    let ambient = coordinator
        .request_refresh(&account, terminal.generation, false, 1_002)
        .unwrap();
    assert_eq!(ambient.generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn coordinator_refresh_all_deduplicates_canonical_accounts() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let account_a = capability("account-a");
    let account_b = capability("account-b");
    let results = coordinator.request_refresh_all(
        [
            (account_a.clone(), 0),
            (account_a.clone(), 0),
            (account_b.clone(), 0),
        ],
        true,
        1_000,
    );
    assert_eq!(results.len(), 2);
    executor.wait_started(2);
    executor.release(2);
    drop(join_ok(&coordinator, &account_a, 1, 1_001));
    drop(join_ok(&coordinator, &account_b, 1, 1_001));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn coordinator_empty_result_is_failure_and_preserves_last_good() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let account = capability("account-a");
    coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    executor.release(1);
    drop(join_ok(&coordinator, &account, 1, 1_001));

    executor.set_outcome(ProviderProbeOutcome::success(
        FocusedUsageView::unavailable("empty", 1_002),
    ));
    coordinator
        .request_refresh(&account, 1, true, 1_002)
        .unwrap();
    executor.wait_started(2);
    executor.release(1);
    let terminal = join_ok(&coordinator, &account, 2, 1_003);
    assert_eq!(terminal.phase, UsageRefreshPhase::Failed);
    assert_eq!(
        terminal.snapshot.unwrap().buckets[0].remaining_percent,
        Some(80)
    );
}

#[test]
fn coordinator_stale_and_error_success_results_schedule_retry() {
    for status in [UsageSnapshotStatus::Stale, UsageSnapshotStatus::Error] {
        let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(
            quota_view(1_000, 80),
        )));
        #[expect(
            clippy::clone_on_ref_ptr,
            reason = "coerce concrete executor to shared trait object"
        )]
        let provider_executor: Arc<dyn UsageProviderExecutor> = executor.clone();
        let coordinator = UsageCoordinator::new(
            provider_executor,
            Arc::new(MemoryStore::default()),
            UsageCoordinatorConfig::default(),
        );
        let account = capability("account-a");
        let first = coordinator
            .request_refresh(&account, 0, true, 1_000)
            .unwrap();
        let first = join_ok(&coordinator, &account, first.generation, 1_001);

        let mut failed_view = quota_view(1_002, 80);
        failed_view.status = status;
        failed_view.last_error = Some("provider result is not current".to_owned());
        executor.set_outcome(ProviderProbeOutcome::success(failed_view));
        let second = coordinator
            .request_refresh(&account, first.generation, true, 1_002)
            .unwrap();
        let failed = join_ok(&coordinator, &account, second.generation, 1_003);

        assert_eq!(failed.phase, UsageRefreshPhase::Failed);
        assert_eq!(
            failed.error.as_ref().map(|error| error.kind),
            Some(UsageCoordinationErrorKind::ProviderUnavailable)
        );
        assert!(failed.retry_at_epoch.is_some());
        assert_eq!(
            failed
                .snapshot
                .as_ref()
                .and_then(|view| view.buckets.first())
                .and_then(|bucket| bucket.remaining_percent),
            Some(80)
        );
    }
}

#[test]
fn coordinator_unsupported_result_stays_unsupported_without_quota() {
    let mut view = quota_view(1_000, 80);
    view.status = UsageSnapshotStatus::Unsupported;
    view.buckets.clear();
    view.source = UsageSource::None;
    view.confidence = UsageConfidence::None;
    let executor = Arc::new(ImmediateExecutor::new(ProviderProbeOutcome::success(view)));
    #[expect(
        clippy::clone_on_ref_ptr,
        reason = "coerce concrete executor to shared trait object"
    )]
    let provider_executor: Arc<dyn UsageProviderExecutor> = executor.clone();
    let coordinator = UsageCoordinator::new(
        provider_executor,
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let account = capability("unsupported-account");
    let queued = coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    let result = join_ok(&coordinator, &account, queued.generation, 1_001);

    assert_eq!(result.phase, UsageRefreshPhase::Completed);
    assert_eq!(result.retry_at_epoch, None);
    let snapshot = result.snapshot.expect("unsupported snapshot");
    assert_eq!(snapshot.status, UsageSnapshotStatus::Unsupported);
    assert!(snapshot.buckets.is_empty());
}

#[test]
fn generation_view_exposes_the_latest_account_retry_deadline() {
    let account = capability("account-a");
    let mut envelope = AccountStateEnvelope::idle(account);
    envelope.rate_limit_deadline_epoch = Some(2_000);
    envelope.retry_deadline_epoch = Some(3_000);
    assert_eq!(generation_view(&envelope).retry_at_epoch, Some(3_000));

    envelope.rate_limit_deadline_epoch = Some(4_000);
    envelope.retry_deadline_epoch = Some(3_000);
    assert_eq!(generation_view(&envelope).retry_at_epoch, Some(4_000));
}

#[test]
fn coordinator_unavailable_or_corrupt_state_makes_zero_provider_calls() {
    for error in [StateStoreError::Unavailable, StateStoreError::Corrupt] {
        let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
            quota_view(1_000, 80),
        )));
        let store = Arc::new(MemoryStore::default());
        *store.load_error.lock().unwrap() = Some(error);
        let coordinator = coordinator(
            Arc::clone(&executor),
            store,
            UsageCoordinatorConfig::default(),
        );
        let result = coordinator.request_refresh(&capability("account-a"), 0, true, 1_000);
        result.unwrap_err();
        assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn coordinator_capsule_capability_rejects_non_forwarded_account() {
    let account_a = capability("account-a");
    let account_b = capability("account-b");
    let allowlist = UsageCapabilitySet::new([account_a.clone()]);
    allowlist.authorize(&account_a).unwrap();
    let error = allowlist.authorize(&account_b).unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
}

#[test]
fn coordinator_capsule_surface_rejects_ambiguous_accounts() {
    let account_a = capability("account-a");
    let account_b = capability("account-b");
    let allowlist = UsageCapabilitySet::new([account_a, account_b]);

    let error = allowlist.resolve_surface("claude").unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
}

#[test]
fn coordinator_failure_shares_retry_deadline_and_last_good() {
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(
        quota_view(1_000, 80),
    )));
    let coordinator = coordinator(
        Arc::clone(&executor),
        Arc::new(MemoryStore::default()),
        UsageCoordinatorConfig::default(),
    );
    let account = capability("account-a");
    coordinator
        .request_refresh(&account, 0, true, 1_000)
        .unwrap();
    executor.wait_started(1);
    executor.release(1);
    drop(join_ok(&coordinator, &account, 1, 1_001));
    executor.set_outcome(ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::RateLimited,
        message: "provider rate limited".into(),
        retry_at_epoch: Some(2_000),
    });
    coordinator
        .request_refresh(&account, 1, true, 1_002)
        .unwrap();
    executor.wait_started(2);
    executor.release(1);
    let failed = join_ok(&coordinator, &account, 2, 1_003);
    assert_eq!(failed.retry_at_epoch, Some(2_000));
    assert!(failed.snapshot.is_some());
    let suppressed = coordinator
        .request_refresh(&account, 2, true, 1_004)
        .unwrap();
    assert_eq!(suppressed.generation, 2);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
}
