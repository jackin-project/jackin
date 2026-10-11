// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_usage_coordinator::policy::UsageActivity;
use jackin_usage_coordinator::{FileAccountStateStore, UsageCoordinator, UsageCoordinatorConfig};

#[test]
fn wall_clock_wake_recalculates_cadence_without_provider_work() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let executor_trait: Arc<dyn UsageProviderExecutor> = Arc::<CountingExecutor>::clone(&executor);
    let store = Arc::new(FileAccountStateStore::at(temp.path().join("accounts")));
    let coordinator =
        UsageCoordinator::new(executor_trait, store, UsageCoordinatorConfig::default());
    let account = capability();
    coordinator
        .set_activity(&account, UsageActivity::DirectInteraction, false, 1_000)
        .unwrap();

    assert!(!serve::wall_clock_wake_detected(
        1_000,
        Duration::from_secs(1),
        1_001,
    ));
    assert!(serve::wall_clock_wake_detected(
        1_000,
        Duration::from_secs(1),
        37_000,
    ));

    assert_eq!(coordinator.note_wake(37_000), 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    assert!(coordinator.poll_due(37_000).is_empty());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    assert!(coordinator.next_due_epoch().is_some_and(|due| due > 37_000));
}
