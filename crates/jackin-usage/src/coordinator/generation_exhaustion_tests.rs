// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn exhausted_envelope(account: &UsageAccountCapability) -> AccountStateEnvelope {
    let mut envelope = AccountStateEnvelope::idle(account.clone());
    envelope.generation = u64::MAX;
    envelope.phase = UsageRefreshPhase::Completed;
    envelope.last_good = Some(quota_view(1_000, 80));
    envelope.terminal_result = envelope.last_good.clone();
    envelope
}

#[test]
fn exhausted_revoke_replaces_same_generation_terminal_history() {
    let account = capability("exhausted-revoke");
    let mut entry = AccountEntry::new(exhausted_envelope(&account), false, 1_000, None);
    revoke_entry(&mut entry, 1_001);
    assert!(entry.revoked);
    assert_eq!(entry.history.len(), 1);
    let terminal = entry.history.front().unwrap();
    assert_eq!(terminal.generation, u64::MAX);
    assert_eq!(terminal.phase, UsageRefreshPhase::Failed);
    assert_eq!(terminal.error.as_ref().unwrap().kind, UsageCoordinationErrorKind::Unavailable);
}

#[test]
fn exhausted_manual_refresh_retains_snapshot_and_never_dispatches() {
    let account = capability("exhausted-manual");
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(quota_view(1_001, 70))));
    let store = Arc::new(MemoryStore::default());
    store.states.lock().unwrap().insert(account.clone(), exhausted_envelope(&account));
    let coordinator = coordinator(Arc::clone(&executor), Arc::clone(&store), UsageCoordinatorConfig::default());
    // Load the original MAX completion into history before replacing it.
    assert_eq!(coordinator.current(&account, 1_001).unwrap().phase, UsageRefreshPhase::Completed);
    for now in [1_002, 1_003] {
        let error = coordinator.request_refresh(&account, u64::MAX, true, now).unwrap_err();
        assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    }
    let terminal = join_ok(&coordinator, &account, u64::MAX, 1_003);
    assert_eq!(terminal.phase, UsageRefreshPhase::Failed);
    assert_eq!(terminal.error.unwrap().kind, UsageCoordinationErrorKind::Unavailable);
    assert!(terminal.snapshot.is_some());
    assert_eq!(terminal.generation, u64::MAX);
    let polled = coordinator.poll_due(2_000);
    assert_eq!(polled.len(), 1);
    assert_eq!(polled[0].error.as_ref().unwrap().kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    assert_eq!(store.states.lock().unwrap()[&account].generation, u64::MAX);
}

#[test]
fn exhausted_restore_changed_catalog_stays_terminal_across_restart() {
    let account = capability("exhausted-restore");
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(quota_view(1_001, 70))));
    let store = Arc::new(MemoryStore::default());
    let mut envelope = exhausted_envelope(&account);
    envelope.accepted_catalog_entry = Some(catalog_entry(&account, "old"));
    store.states.lock().unwrap().insert(account.clone(), envelope);
    for _ in 0..2 {
        let coordinator = UsageCoordinator::with_catalog(Arc::<GateExecutor>::clone(&executor), Arc::<MemoryStore>::clone(&store), UsageCoordinatorConfig::default(), [catalog_entry(&account, "new")]);
        let terminal = join_ok(&coordinator, &account, u64::MAX, 1_002);
        assert_eq!(terminal.phase, UsageRefreshPhase::Failed);
        assert_eq!(terminal.error.unwrap().kind, UsageCoordinationErrorKind::Unavailable);
        assert!(terminal.snapshot.is_none());
        assert_eq!(coordinator.request_refresh(&account, u64::MAX, true, 1_003).unwrap_err().kind, UsageCoordinationErrorKind::Unavailable);
    }
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn exhausted_catalog_reset_wakes_owned_generation_and_fences_late_result() {
    let account = capability("exhausted-reset");
    let executor = Arc::new(GateExecutor::new(ProviderProbeOutcome::success(quota_view(1_001, 70))));
    let store = Arc::new(MemoryStore::default());
    let mut envelope = exhausted_envelope(&account);
    envelope.generation = u64::MAX - 1;
    envelope.accepted_catalog_entry = Some(catalog_entry(&account, "old"));
    store.states.lock().unwrap().insert(account.clone(), envelope);
    let coordinator = Arc::new(UsageCoordinator::with_catalog(Arc::<GateExecutor>::clone(&executor), Arc::<MemoryStore>::clone(&store), UsageCoordinatorConfig::default(), [catalog_entry(&account, "old")]));
    assert_eq!(coordinator.request_refresh(&account, u64::MAX - 1, true, 1_002).unwrap().generation, u64::MAX);
    executor.wait_started(1);
    let waiter = Arc::clone(&coordinator);
    let waiter_account = account.clone();
    let joined = std::thread::spawn(move || join_ok(&waiter, &waiter_account, u64::MAX, 1_003));
    coordinator.reconcile_catalog([catalog_entry(&account, "new")], 1_003).unwrap();
    let terminal = joined.join().unwrap();
    assert_eq!(terminal.phase, UsageRefreshPhase::Failed);
    assert_eq!(terminal.error.unwrap().kind, UsageCoordinationErrorKind::Unavailable);
    assert!(terminal.snapshot.is_none());
    executor.release(1);
    executor.wait_idle();
    drop(coordinator);
    let restarted = UsageCoordinator::with_catalog(Arc::<GateExecutor>::clone(&executor), Arc::<MemoryStore>::clone(&store), UsageCoordinatorConfig::default(), [catalog_entry(&account, "new")]);
    assert_eq!(restarted.request_refresh(&account, u64::MAX, true, 1_004).unwrap_err().kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(join_ok(&restarted, &account, u64::MAX, 1_004).phase, UsageRefreshPhase::Failed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}
