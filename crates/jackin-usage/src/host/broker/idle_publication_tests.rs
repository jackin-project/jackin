// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Observe autonomous publication through durable state, never broker traffic.

use std::sync::atomic::{AtomicUsize, Ordering};

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageAccountIdentity, UsageCanonicalAccountIdentity,
    UsageCanonicalAccountSubject, UsageConfidence, UsageSeverity, UsageSnapshotStatus, UsageSource,
};
use jackin_protocol::usage_broker::{
    UsageFreshnessPhaseV2, UsageLifecycleV2, UsageProjectionRefreshStateV2, UsageProjectionV2,
};

use super::*;

struct HeldTerminalExecutor {
    started: mpsc::SyncSender<()>,
    release: Mutex<mpsc::Receiver<()>>,
    calls: AtomicUsize,
    entry: UsageCatalogEntry,
    fail: bool,
}

impl UsageProviderExecutor for HeldTerminalExecutor {
    fn probe(&self, capability: &UsageAccountCapability, _: u64) -> ProviderProbeOutcome {
        assert_eq!(capability, &self.entry.capability);
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        if self.fail {
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::Unavailable,
                message: "terminal idle failure".to_owned(),
                retry_at_epoch: None,
            }
        } else {
            let mut view = FocusedUsageView::unavailable("claude", chrono::Utc::now().timestamp());
            view.status = UsageSnapshotStatus::Fresh;
            view.source = UsageSource::ProviderApi;
            view.confidence = UsageConfidence::Authoritative;
            view.canonical_identity = self.entry.canonical_identity.clone();
            view.account_identity = Some(UsageAccountIdentity {
                account_id: capability.account_id.clone(),
                surface_id: capability.surface_id.clone(),
                source_revision: Some(self.entry.revision.clone()),
            });
            view.buckets = vec![QuotaBucketView {
                count_quota: None,
                label: "Weekly".to_owned(),
                used_label: None,
                limit_label: None,
                remaining_percent: Some(75),
                reset_label: None,
                resets_at: None,
                status_slot: None,
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
                used_money: None,
                limit_money: None,
                remaining_money: None,
                severity: UsageSeverity::Normal,
            }];
            ProviderProbeOutcome::success(view)
        }
    }
}

fn await_durable_projection(
    store: &FileProjectionStateStore,
    accept: impl Fn(&UsageProjectionV2) -> bool,
) -> UsageProjectionV2 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(envelope) = store.load().unwrap()
            && accept(&envelope.projection)
        {
            envelope.projection.validate().unwrap();
            return envelope.projection;
        }
        assert!(
            Instant::now() < deadline,
            "autonomous durable publication timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn terminal_publication_without_client_traffic(fail: bool) {
    let temp = tempfile::tempdir().unwrap();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let entry = UsageCatalogEntry {
        capability: UsageAccountCapability {
            account_id: "idle-terminal-route".to_owned(),
            surface_id: "claude".to_owned(),
        },
        canonical_identity: Some(UsageCanonicalAccountIdentity {
            surface_id: "claude".to_owned(),
            subject: UsageCanonicalAccountSubject::ProviderId("idle-terminal-subject".to_owned()),
        }),
        provenance_count: 1,
        revision: "idle-terminal-revision".to_owned(),
    };
    let executor = Arc::new(HeldTerminalExecutor {
        started: started_tx,
        release: Mutex::new(release_rx),
        calls: AtomicUsize::new(0),
        entry: entry.clone(),
        fail,
    });
    let mut config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    config.idle_exit = Duration::from_secs(30);
    let client = ensure_usage_broker_with_executor(config, executor.clone()).unwrap();
    client
        .reconcile_catalog("idle-catalog".to_owned(), vec![entry.clone()])
        .unwrap();
    let active = client.subscribe(entry.capability).unwrap();
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let before = await_durable_projection(&store, |projection| {
        projection.refresh_state == UsageProjectionRefreshStateV2::Refreshing
            && projection
                .providers
                .iter()
                .flat_map(|provider| &provider.accounts)
                .any(|account| {
                    account.freshness.generation == active.generation
                        && account.freshness.phase == UsageFreshnessPhaseV2::Refreshing
                })
    });
    // Ensure the active state settled across a cadence boundary before release.
    std::thread::sleep(PUBLISH_TICK * 2);
    assert_eq!(store.load().unwrap().unwrap().projection, before);
    release_tx.send(()).unwrap();

    // No broker requests, joins, or explicit publisher calls after release.
    // A live service must publish the final completion on its own.
    let terminal_phase = if fail {
        UsageFreshnessPhaseV2::Failed
    } else {
        UsageFreshnessPhaseV2::Current
    };
    let settled = await_durable_projection(&store, |projection| {
        projection.refresh_state == UsageProjectionRefreshStateV2::Idle
            && projection.broker_generation > before.broker_generation
            && projection
                .providers
                .iter()
                .flat_map(|provider| &provider.accounts)
                .any(|account| {
                    account.freshness.generation == active.generation
                        && account.freshness.phase == terminal_phase
                })
    });
    let account = &settled.providers[0].accounts[0];
    if fail {
        assert_eq!(account.lifecycle, UsageLifecycleV2::Unavailable);
        assert!(account.windows.is_empty());
        assert!(
            account
                .issues
                .iter()
                .any(|issue| issue.message == "terminal idle failure")
        );
    } else {
        assert!(!account.windows.is_empty());
        assert!(account.issues.is_empty());
    }
    for _ in 0..3 {
        std::thread::sleep(PUBLISH_TICK);
        assert_eq!(store.load().unwrap().unwrap().projection, settled);
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn final_success_publishes_while_service_idle_without_client_traffic() {
    terminal_publication_without_client_traffic(false);
}

#[test]
fn final_failure_publishes_while_service_idle_without_client_traffic() {
    terminal_publication_without_client_traffic(true);
}
