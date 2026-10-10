// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, StatusSlot, UsageConfidence, UsageSeverity,
    UsageSnapshotStatus, UsageSource,
};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageFreshnessPhaseV1,
    UsageProjectionRefreshStateV1, UsageRefreshPhase,
};
use jackin_protocol::usage_monitor::{
    MonitorAccountBindingInput, MonitorConfig, MonitorOperation, MonitorProvider, MonitorPurpose,
    MonitorReply, MonitorScope,
};
use jackin_usage_coordinator::{
    FileAccountStateStore, FileProjectionStateStore, ProviderProbeOutcome, UsageCoordinator,
    UsageCoordinatorConfig, UsageProviderExecutor,
};

const COLLECTOR_ACCOUNT_ID: &str = "collector-test-account";
const SOURCE_ID_PREFIX: &str = "a";

fn source_id() -> String {
    SOURCE_ID_PREFIX.repeat(64)
}

struct GatedExecutor {
    started: Sender<()>,
    release: Mutex<Receiver<()>>,
    calls: AtomicUsize,
}

impl UsageProviderExecutor for GatedExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let _ignored = self.started.send(());
        if let Ok(release) = self.release.lock() {
            let _ignored = release.recv();
        }
        ProviderProbeOutcome::success(provider_view(NOW + 2))
    }
}

struct RetryAfterExecutor {
    calls: AtomicUsize,
}

impl UsageProviderExecutor for RetryAfterExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        match self.calls.fetch_add(1, Ordering::SeqCst) + 1 {
            1 => ProviderProbeOutcome::success(provider_view(NOW)),
            2 => ProviderProbeOutcome::Failure {
                kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::RateLimited,
                message: "fixture retry after".to_owned(),
                retry_at_epoch: Some(NOW + 1_200),
            },
            _ => ProviderProbeOutcome::success(provider_view(NOW + 1_200)),
        }
    }
}

fn provider_view(now_epoch: i64) -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("fixture", now_epoch);
    view.focused_agent = Some("claude".to_owned());
    view.focused_provider = Some("Claude".to_owned());
    view.account.provider_label = "Anthropic".to_owned();
    view.account.account_label = "fixture-account".to_owned();
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.buckets = [
        ("Session", StatusSlot::Session, 64, now_epoch + 3_600),
        (
            "Weekly",
            StatusSlot::Weekly,
            72,
            now_epoch + 7 * 24 * 60 * 60,
        ),
    ]
    .into_iter()
    .map(
        |(label, status_slot, remaining, resets_at)| QuotaBucketView {
            label: label.to_owned(),
            used_label: None,
            limit_label: None,
            remaining_percent: Some(remaining),
            reset_label: None,
            resets_at: Some(resets_at),
            status_slot: Some(status_slot),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        },
    )
    .collect();
    view.last_error = None;
    view
}

struct CollectorHarness {
    store: Arc<MonitorStore>,
    coordinator: Arc<UsageCoordinator>,
    publisher: publish::ProjectionPublisher,
    temp: tempfile::TempDir,
    capability: UsageAccountCapability,
}

impl CollectorHarness {
    fn new(executor: Arc<dyn UsageProviderExecutor>) -> Self {
        let temp = tempfile::tempdir().expect("temporary collector state");
        let source_id = source_id();
        let capability = crate::service::claude_usage_capability_for_source_id(&source_id);
        let catalog = vec![UsageCatalogEntry {
            revision: "collector-test-catalog-revision".to_owned(),
            capability: capability.clone(),
        }];
        let coordinator = Arc::new(UsageCoordinator::with_catalog(
            executor,
            Arc::new(FileAccountStateStore::at(temp.path().join("accounts"))),
            UsageCoordinatorConfig::default(),
            catalog.clone(),
        ));
        let publisher = publish::ProjectionPublisher::new(
            Arc::clone(&coordinator),
            Arc::new(Mutex::new(crate::projection::empty_projection(
                "collector-test",
            ))),
            FileProjectionStateStore::under_data_dir(temp.path()),
        )
        .with_catalog(catalog);
        let store = Arc::new(MonitorStore::open(temp.path()).expect("open monitor state"));
        store.set_experimental_collector_source(Some(source_id));
        Self {
            store,
            coordinator,
            publisher,
            temp,
            capability,
        }
    }

    fn start_approved_observer(&self) -> String {
        let source_id = source_id();
        let binding = match self
            .store
            .operate(
                MonitorOperation::BindAccount {
                    binding: MonitorAccountBindingInput {
                        provider: MonitorProvider::Claude,
                        account_id: COLLECTOR_ACCOUNT_ID.to_owned(),
                        operator_label: "collector-test-operator".to_owned(),
                        operator_confirmed: true,
                        provider_account_id: Some(source_id.clone()),
                        experimental_collector_approved: true,
                    },
                },
                NOW,
            )
            .expect("bind explicitly approved source capability")
        {
            MonitorReply::AccountBound { binding } => binding,
            other => panic!("expected account-bound reply, got {other:?}"),
        };
        assert_eq!(
            self.store.collection_accounts(),
            vec![source_id],
            "only the exact approved foreground source enters collector polling"
        );

        let reply = self
            .store
            .operate(
                MonitorOperation::Start {
                    config: MonitorConfig {
                        provider: MonitorProvider::Claude,
                        purpose: MonitorPurpose::ObserveOnly,
                        scope: MonitorScope::BoundAccount {
                            binding_id: binding.binding_id,
                            binding_revision: binding.revision,
                            session_id: None,
                        },
                        goal_id: None,
                        expected_model: None,
                        policy_revision: None,
                        experimental_collector: true,
                    },
                    idempotency_key: "approved-collector-observer".to_owned(),
                },
                NOW,
            )
            .expect("start opted-in observer");
        let MonitorReply::Started { status } = reply else {
            panic!("expected collector observer to start, got {reply:?}");
        };
        status.monitor_id
    }

    fn ticker_state(&self) -> TickerState {
        TickerState {
            last_sample: ClockSample {
                wall_epoch: NOW,
                monotonic_elapsed: Duration::ZERO,
            },
            last_observed_projection_id: self
                .publisher
                .current_projection()
                .expect("read initial projection")
                .projection_id
                .into(),
            last_ticked_wake: None,
            retry: None,
            last_collection_check: None,
            last_publish_attempt: None,
            observation_retry_after: None,
        }
    }

    fn monitor_state_file(&self) -> std::path::PathBuf {
        self.temp
            .path()
            .join(crate::BROKER_DIR)
            .join("monitor")
            .join("state.json")
    }
}

#[test]
fn ticker_polls_approved_source_and_publishes_terminal_result_while_idle() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::sync_channel(0);
    let executor = Arc::new(GatedExecutor {
        started: started_sender,
        release: Mutex::new(release_receiver),
        calls: AtomicUsize::new(0),
    });
    let executor_trait: Arc<dyn UsageProviderExecutor> = Arc::<GatedExecutor>::clone(&executor);
    let harness = CollectorHarness::new(executor_trait);
    harness.start_approved_observer();
    let mut ticker_state = harness.ticker_state();

    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW,
            monotonic_elapsed: Duration::from_millis(200),
        },
        &mut ticker_state,
    );
    started_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("approved observer dispatches through the ticker");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let generation = harness
        .coordinator
        .current(&harness.capability, NOW)
        .expect("load active collector generation")
        .generation;
    let updating = harness
        .publisher
        .current_projection()
        .expect("read updating projection");
    assert_eq!(
        updating.refresh_state,
        UsageProjectionRefreshStateV1::Refreshing
    );

    let state_file = harness.monitor_state_file();
    let backup_file = state_file.with_extension("state-backup");
    fs::rename(&state_file, &backup_file).expect("move state file for persistence fault");
    symlink(&backup_file, &state_file).expect("install test-only state symlink");
    release_sender.send(()).expect("release provider result");
    let terminal = harness
        .coordinator
        .join_generation(
            &harness.capability,
            generation,
            Duration::from_secs(1),
            NOW + 2,
        )
        .expect("wait for terminal provider result");
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert!(harness.coordinator.is_idle());

    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 2,
            monotonic_elapsed: Duration::from_millis(1_200),
        },
        &mut ticker_state,
    );
    let completed = harness
        .publisher
        .current_projection()
        .expect("terminal state publishes without a client refresh");
    assert_eq!(completed.refresh_state, UsageProjectionRefreshStateV1::Idle);
    let account = completed
        .providers
        .iter()
        .flat_map(|provider| &provider.accounts)
        .find(|account| account.canonical_account_id == harness.capability.account_id)
        .expect("hashed capability is published");
    assert_eq!(account.freshness.phase, UsageFreshnessPhaseV1::Current);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        ticker_state.last_observed_projection_id.as_deref(),
        Some(updating.projection_id.as_str()),
        "failed monitor persistence must leave the terminal projection pending"
    );
    let retry_after = ticker_state.observation_retry_after;
    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 3,
            monotonic_elapsed: Duration::from_millis(1_400),
        },
        &mut ticker_state,
    );
    assert_eq!(ticker_state.observation_retry_after, retry_after);

    fs::remove_file(&state_file).expect("remove test-only symlink");
    fs::rename(&backup_file, &state_file).expect("restore monitor state file");
    publisher_tick_step(
        &harness.publisher,
        &harness.coordinator,
        &harness.store,
        ClockSample {
            wall_epoch: NOW + 3,
            monotonic_elapsed: Duration::from_millis(2_200),
        },
        &mut ticker_state,
    );
    assert_eq!(
        ticker_state.last_observed_projection_id.as_deref(),
        Some(completed.projection_id.as_str()),
        "successful persistence records the completed projection"
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn ticker_collector_preserves_minimum_attempt_floor_and_retry_after() {
    let executor = Arc::new(RetryAfterExecutor {
        calls: AtomicUsize::new(0),
    });
    let executor_trait: Arc<dyn UsageProviderExecutor> =
        Arc::<RetryAfterExecutor>::clone(&executor);
    let harness = CollectorHarness::new(executor_trait);
    harness.start_approved_observer();
    let mut ticker_state = harness.ticker_state();

    let tick = |wall_epoch, monotonic_elapsed, state: &mut TickerState| {
        publisher_tick_step(
            &harness.publisher,
            &harness.coordinator,
            &harness.store,
            ClockSample {
                wall_epoch,
                monotonic_elapsed,
            },
            state,
        );
    };
    tick(NOW, Duration::from_millis(200), &mut ticker_state);
    let first_generation = harness
        .coordinator
        .current(&harness.capability, NOW)
        .expect("first collector generation")
        .generation;
    harness
        .coordinator
        .join_generation(
            &harness.capability,
            first_generation,
            Duration::from_secs(1),
            NOW,
        )
        .expect("join initial successful provider result");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    tick(NOW + 299, Duration::from_secs(299), &mut ticker_state);
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        1,
        "a successful Claude result retains the 300-second attempt floor"
    );
    tick(NOW + 300, Duration::from_secs(300), &mut ticker_state);
    let limited_generation = harness
        .coordinator
        .current(&harness.capability, NOW + 300)
        .expect("second collector generation")
        .generation;
    harness
        .coordinator
        .join_generation(
            &harness.capability,
            limited_generation,
            Duration::from_secs(1),
            NOW + 300,
        )
        .expect("join rate-limited provider result");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    tick(NOW + 1_199, Duration::from_secs(1_199), &mut ticker_state);
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        2,
        "Retry-After remains the maximum dispatch deadline"
    );
    tick(NOW + 1_200, Duration::from_secs(1_200), &mut ticker_state);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 3);
}
