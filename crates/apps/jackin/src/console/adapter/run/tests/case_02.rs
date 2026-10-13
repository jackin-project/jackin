// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn console_usage_reads_the_broker_publication_without_client_probe_work() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCoordinationErrorKind};
    use jackin_usage::coordinator::{ProviderProbeOutcome, UsageProviderExecutor};
    use jackin_usage::host::{UsageBrokerConfig, ensure_usage_broker_with_executor};

    struct CountingExecutor(AtomicUsize);

    impl UsageProviderExecutor for CountingExecutor {
        fn probe(
            &self,
            _capability: &UsageAccountCapability,
            _generation: u64,
        ) -> ProviderProbeOutcome {
            self.0.fetch_add(1, Ordering::SeqCst);
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::Unavailable,
                message: "unexpected probe in passive-read fixture".to_owned(),
                retry_at_epoch: None,
            }
        }
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let executor = Arc::new(CountingExecutor(AtomicUsize::new(0)));
    let executor_trait: Arc<dyn UsageProviderExecutor> = Arc::<CountingExecutor>::clone(&executor);
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(dir.path().to_owned()),
        executor_trait,
    )
    .expect("fake broker");
    let before = client.current_projection().expect("initial publication");

    let state = load_console_usage_projection(&client, false, false).expect("Console publication");

    let after = client.current_projection().expect("published projection");
    assert_eq!(state.canonical_projection.as_ref(), Some(&after));
    assert_eq!(before.projection_id, after.projection_id);
    assert_eq!(executor.0.load(Ordering::SeqCst), 0);
}

#[test]
fn console_startup_refresh_stays_inside_the_fake_broker() {
    use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCoordinationErrorKind};
    use jackin_usage::coordinator::{ProviderProbeOutcome, UsageProviderExecutor};
    use jackin_usage::host::{UsageBrokerConfig, ensure_usage_broker_with_executor};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingExecutor(AtomicUsize);

    impl UsageProviderExecutor for CountingExecutor {
        fn probe(
            &self,
            _capability: &UsageAccountCapability,
            _generation: u64,
        ) -> ProviderProbeOutcome {
            self.0.fetch_add(1, Ordering::SeqCst);
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::ProviderUnavailable,
                message: "fake broker fixture has no provider adapter".to_owned(),
                retry_at_epoch: None,
            }
        }
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let executor = Arc::new(CountingExecutor(AtomicUsize::new(0)));
    let executor_trait: Arc<dyn UsageProviderExecutor> = Arc::<CountingExecutor>::clone(&executor);
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(dir.path().to_owned()),
        executor_trait,
    )
    .expect("fake broker");
    let snapshot = load_console_usage_projection(&client, false, true)
        .expect("Console startup broker refresh");
    assert!(snapshot.canonical_projection.is_some());
    // This fake broker has no broker-owned catalog refresher. Console can
    // request refresh, but it cannot inject catalog entries or invoke a
    // provider itself.
    assert_eq!(executor.0.load(Ordering::SeqCst), 0);
}
