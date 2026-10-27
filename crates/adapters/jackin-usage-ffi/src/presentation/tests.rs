use super::*;
use jackin_protocol::control::{
    CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit, MeterPercentPoints,
    PercentObservationOrigin, ProviderQuotaState, RawPercentPoints, UsageMeterSide,
};
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageFreshnessV1, UsageIdentityKindV1, UsageLifecycleV1, UsageLimitWindowV1,
    UsageMembershipStateV1, UsageProjectionRefreshStateV1, UsageProjectionSchemaV2,
    UsageProjectionV1, UsageProviderV1, UsageQuotaStateV1, UsageWindowCategoryV1,
};
use jackin_protocol::usage_broker::{UsageFreshnessPhaseV1, UsageMetricScopeV1};
use jackin_protocol::usage_monitor::{MonitorOperation, MonitorReply};
use jackin_usage::coordinator::{
    FileProjectionStateStore, ProjectionAlias, ProjectionStateEnvelope,
};
use jackin_usage::host::{UsageBrokerClient, UsageBrokerConfig, run_usage_monitor_service};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

static FIXTURE_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn spend_cap_labels_preserve_exact_money_digits() {
    let group = UsageMetricGroupV1 {
        group_id: "spend".to_owned(),
        rank: 0,
        kind: UsageMetricGroupKindV1::SpendCap,
        label: "Spend cap".to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: 0,
        last_success_at_epoch: None,
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(100_000_000_000_000_001, "USD", 18)),
            spent: Some(Money::new(100_000_000_000_000_000, "USD", 18)),
            remaining: Some(Money::new(1, "USD", 18)),
        },
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    };

    let bucket = spend_cap_bucket(&group).expect("spend cap bucket");
    assert_eq!(bucket.used_label.as_deref(), Some("$0.100000000000000000"));
    assert_eq!(bucket.limit_label.as_deref(), Some("$0.100000000000000001"));
    assert_eq!(
        metric_group_summary(&group),
        "spent $0.100000000000000000 · cap $0.100000000000000001 · remaining $0.000000000000000001"
    );
}

#[test]
fn spend_cap_labels_preserve_mixed_money_scales() {
    let group = UsageMetricGroupV1 {
        group_id: "spend-mixed-scale".to_owned(),
        rank: 0,
        kind: UsageMetricGroupKindV1::SpendCap,
        label: "Spend cap".to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: 0,
        last_success_at_epoch: None,
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(100_000_000_000_000_000, "USD", 0)),
            spent: Some(Money::new(999_999_999_999_999_999, "USD", 2)),
            remaining: Some(Money::new(9_000_000_000_000_000_001, "USD", 2)),
        },
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    };
    group
        .validate(0)
        .expect("mixed-scale spend group validates");

    let bucket = spend_cap_bucket(&group).expect("spend cap bucket");
    assert_eq!(bucket.limit_label.as_deref(), Some("$100000000000000000"));
    assert_eq!(bucket.used_label.as_deref(), Some("$9999999999999999.99"));
    assert_eq!(
        metric_group_summary(&group),
        "spent $9999999999999999.99 · cap $100000000000000000 · remaining $90000000000000000.01"
    );
}

#[test]
fn public_snapshot_detail_preserves_count_observations() {
    let windows = count_fixture_windows();
    let mut fixture = LocalUsageFixture::start(windows.clone());
    let view = fixture
        .bridge
        .snapshot("openrouter".to_owned())
        .expect("public OpenRouter snapshot");

    assert_snapshot_identity(&view, &windows);
    assert_state_controls(&view);
    assert_compact_count_labels(&view);
    assert_detail_count_labels(&view);

    fixture
        .stop_and_join()
        .expect("local usage service stops and joins");
}

fn assert_snapshot_identity(view: &UsageViewDto, windows: &[UsageLimitWindowV1]) {
    assert_eq!(view.focused_provider.as_deref(), Some("openrouter"));
    assert_eq!(view.account_label, "Fixture account");
    assert_eq!(view.status, "stale");
    assert_eq!(view.buckets.len(), windows.len());
    let detail_rows = view
        .detail_presentation
        .rows
        .iter()
        .filter(|row| row.kind == "bucket")
        .collect::<Vec<_>>();
    assert_eq!(detail_rows.len(), windows.len());
    for (row, window) in detail_rows.iter().zip(windows) {
        assert_eq!(row.row_id, window.window_id);
        assert_eq!(row.label, window.label);
    }
    for (bucket, window) in view.buckets.iter().zip(windows) {
        assert_count_observation(bucket, window.count_quota.as_ref());
    }
}

fn assert_count_observation(bucket: &QuotaBucketDto, expected: Option<&CountQuota>) {
    let actual = bucket.count_quota.as_ref();
    assert_eq!(actual.is_some(), expected.is_some());
    let Some(expected) = expected else {
        return;
    };
    let actual = actual.expect("public bucket count observation");
    assert_eq!(actual.used, expected.used);
    assert_eq!(actual.limit, expected.limit);
    assert_eq!(actual.remaining, expected.remaining);
    let unit = match expected.unit {
        CountQuotaUnit::Requests => "requests",
        CountQuotaUnit::Unknown => "unknown",
    };
    let period = match expected.period {
        CountQuotaPeriod::UtcDaily => "utc_daily",
        CountQuotaPeriod::Unknown => "unknown",
    };
    let provenance = match expected.provenance {
        CountQuotaProvenance::ProviderReported => "provider_reported",
    };
    assert_eq!(actual.unit, unit);
    assert_eq!(actual.period, period);
    assert_eq!(actual.provenance, provenance);
}

fn assert_state_controls(view: &UsageViewDto) {
    let state_bucket = view
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Exhausted count")
        .expect("exhausted compact bucket");
    assert_eq!(state_bucket.used_percent, Some(25.0));
    assert_eq!(
        state_bucket.used_percent_origin.as_deref(),
        Some("provider_reported")
    );
    assert_eq!(state_bucket.remaining_percent, Some(75.0));
    assert_eq!(
        state_bucket.remaining_percent_origin.as_deref(),
        Some("derived")
    );
    assert_eq!(
        state_bucket.provider_quota_state.as_deref(),
        Some("exhausted")
    );
    assert_eq!(
        state_bucket
            .count_quota
            .as_ref()
            .and_then(|count| count.used),
        Some(2)
    );
    assert_eq!(
        state_bucket
            .count_quota
            .as_ref()
            .and_then(|count| count.limit),
        None
    );
    assert_eq!(
        state_bucket
            .count_quota
            .as_ref()
            .and_then(|count| count.remaining),
        Some(7)
    );
    assert_eq!(state_bucket.meter_percent, Some(0.0));
    assert_eq!(state_bucket.meter_side.as_deref(), Some("remaining"));
    assert_eq!(
        state_bucket.reset_label.as_deref(),
        Some("Reset at fixture boundary")
    );
    assert_eq!(state_bucket.resets_at, None);
    assert_eq!(state_bucket.pace_label.as_deref(), Some("On pace"));
    assert_eq!(state_bucket.severity, "danger");
    let unlimited_bucket = view
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Unlimited count")
        .expect("unlimited compact bucket");
    assert_eq!(
        unlimited_bucket.provider_quota_state.as_deref(),
        Some("unlimited")
    );
    assert_eq!(unlimited_bucket.used_percent, None);
    assert_eq!(unlimited_bucket.remaining_percent, None);
    assert_eq!(unlimited_bucket.meter_percent, None);
    assert_eq!(unlimited_bucket.meter_side, None);

    let state_row = view
        .detail_presentation
        .rows
        .iter()
        .find(|row| row.row_id == "count-exhausted-partial")
        .expect("exhausted detail row");
    assert_eq!(state_row.meter_percent, Some(0.0));
    assert_eq!(state_row.meter_side.as_deref(), Some("remaining"));
    assert_eq!(state_row.severity, "danger");
    assert_eq!(
        state_row.layout_lines[0].trailing.as_deref(),
        Some("Reset at fixture boundary")
    );
    let unlimited_row = view
        .detail_presentation
        .rows
        .iter()
        .find(|row| row.row_id == "count-unlimited")
        .expect("unlimited detail row");
    assert_eq!(unlimited_row.meter_percent, None);
    assert_eq!(unlimited_row.meter_side, None);
}

fn assert_compact_count_labels(view: &UsageViewDto) {
    assert_eq!(compact_label(view, "Nonadditive count"), "7 requests left");
    assert_eq!(compact_label(view, "9 of 10"), "1 requests left");
    assert_eq!(compact_label(view, "Zero used"), "0 requests used");
    assert_eq!(compact_label(view, "Zero limit"), "0 requests limit");
    assert_eq!(compact_label(view, "Zero remaining"), "0 requests left");
    assert_eq!(compact_label(view, "All unknown"), "fresh");
    assert_eq!(compact_label(view, "No count"), "fresh");
    assert_eq!(compact_label(view, "Unknown vocabulary"), "7 left");
    assert_eq!(
        compact_label(view, "Exhausted count"),
        "Exhausted · Reported used: 25% · Derived remaining: 75% · Reported count (requests/utc_daily): used 2, remaining 7"
    );
    assert_eq!(
        compact_label(view, "Unlimited count"),
        "Unlimited · Reported count (requests/utc_daily): used 2, limit 10, remaining 7"
    );
}

fn assert_detail_count_labels(view: &UsageViewDto) {
    assert_eq!(
        detail_label(view, "count-nonadditive"),
        "Reported count (requests/utc_daily): used 2, limit 10, remaining 7"
    );
    assert_eq!(
        detail_label(view, "count-9-of-10"),
        "Reported count (requests/utc_daily): used 9, limit 10, remaining 1"
    );
    assert_eq!(
        detail_label(view, "count-zero-used"),
        "Reported count (requests/utc_daily): used 0, limit unknown, remaining unknown"
    );
    assert_eq!(
        detail_label(view, "count-zero-limit"),
        "Reported count (requests/utc_daily): used unknown, limit 0, remaining unknown"
    );
    assert_eq!(
        detail_label(view, "count-zero-remaining"),
        "Reported count (requests/utc_daily): used unknown, limit unknown, remaining 0"
    );
    assert_eq!(
        detail_label(view, "count-all-unknown"),
        "fresh · Reported count (requests/utc_daily): used unknown, limit unknown, remaining unknown"
    );
    assert_eq!(detail_label(view, "count-absent"), "fresh");
    assert_eq!(
        detail_label(view, "count-unknown-vocabulary"),
        "Reported count (unknown/unknown): used 2, limit unknown, remaining 7"
    );
    assert_eq!(
        detail_label(view, "count-exhausted-partial"),
        "Exhausted · Reported used: 25% · Derived remaining: 75% · Reported count (requests/utc_daily): used 2, limit unknown, remaining 7"
    );
    assert_eq!(
        detail_label(view, "count-unlimited"),
        "Unlimited · Reported count (requests/utc_daily): used 2, limit 10, remaining 7"
    );
}

fn compact_label<'a>(view: &'a UsageViewDto, label: &str) -> &'a str {
    view.buckets
        .iter()
        .find(|bucket| bucket.label == label)
        .expect("compact bucket")
        .display_label
        .as_str()
}

fn detail_label<'a>(view: &'a UsageViewDto, row_id: &str) -> &'a str {
    view.detail_presentation
        .rows
        .iter()
        .find(|row| row.row_id == row_id)
        .expect("detail row")
        .display_label
        .as_str()
}

fn count_fixture_windows() -> Vec<UsageLimitWindowV1> {
    let cases = [
        (
            "count-nonadditive",
            "Nonadditive count",
            "7 requests left",
            Some(request_count(Some(2), Some(10), Some(7))),
        ),
        (
            "count-9-of-10",
            "9 of 10",
            "1 requests left",
            Some(request_count(Some(9), Some(10), Some(1))),
        ),
        (
            "count-zero-used",
            "Zero used",
            "0 requests used",
            Some(request_count(Some(0), None, None)),
        ),
        (
            "count-zero-limit",
            "Zero limit",
            "0 requests limit",
            Some(request_count(None, Some(0), None)),
        ),
        (
            "count-zero-remaining",
            "Zero remaining",
            "0 requests left",
            Some(request_count(None, None, Some(0))),
        ),
        (
            "count-all-unknown",
            "All unknown",
            "fresh",
            Some(request_count(None, None, None)),
        ),
        ("count-absent", "No count", "fresh", None),
        (
            "count-unknown-vocabulary",
            "Unknown vocabulary",
            "7 left",
            Some(unknown_vocabulary_count()),
        ),
        (
            "count-exhausted-partial",
            "Exhausted count",
            "7 requests left",
            Some(request_count(Some(2), None, Some(7))),
        ),
        (
            "count-unlimited",
            "Unlimited count",
            "7 requests left",
            Some(request_count(Some(2), Some(10), Some(7))),
        ),
    ];
    let mut windows = cases
        .into_iter()
        .enumerate()
        .map(|(rank, (id, label, value, count))| {
            fixture_window(
                u32::try_from(rank).expect("fixture window rank fits u32"),
                id,
                label,
                value,
                count,
            )
        })
        .collect::<Vec<_>>();

    let exhausted = windows
        .iter_mut()
        .find(|window| window.window_id == "count-exhausted-partial")
        .expect("exhausted fixture window");
    exhausted.used_percent = Some(RawPercentPoints::new(25.0).expect("valid raw percentage"));
    exhausted.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    exhausted.remaining_percent = Some(RawPercentPoints::new(75.0).expect("valid raw percentage"));
    exhausted.remaining_percent_origin = Some(PercentObservationOrigin::Derived);
    exhausted.meter_percent = Some(MeterPercentPoints::ZERO);
    exhausted.meter_side = Some(UsageMeterSide::Remaining);
    exhausted.provider_quota_state = Some(ProviderQuotaState::Exhausted);
    exhausted.reset_label = "Reset at fixture boundary".to_owned();
    exhausted.pace_label = Some("On pace".to_owned());
    exhausted.quota_state = UsageQuotaStateV1::Exhausted;

    let unlimited = windows
        .iter_mut()
        .find(|window| window.window_id == "count-unlimited")
        .expect("unlimited fixture window");
    unlimited.provider_quota_state = Some(ProviderQuotaState::Unlimited);
    unlimited.quota_state = UsageQuotaStateV1::NotApplicable;

    windows
}

fn request_count(used: Option<u64>, limit: Option<u64>, remaining: Option<u64>) -> CountQuota {
    CountQuota::new(
        used,
        limit,
        remaining,
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    )
}

fn unknown_vocabulary_count() -> CountQuota {
    CountQuota::new(
        Some(2),
        None,
        Some(7),
        CountQuotaUnit::Unknown,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    )
}

fn fixture_window(
    rank: u32,
    window_id: &str,
    label: &str,
    value_label: &str,
    count_quota: Option<CountQuota>,
) -> UsageLimitWindowV1 {
    UsageLimitWindowV1 {
        window_id: window_id.to_owned(),
        rank,
        category: UsageWindowCategoryV1::Other,
        label: label.to_owned(),
        value_label: value_label.to_owned(),
        reset_label: String::new(),
        remaining_percent: None,
        remaining_percent_origin: None,
        used_percent: None,
        used_percent_origin: None,
        meter_percent: None,
        meter_side: None,
        count_quota,
        provider_quota_state: None,
        money_used_percent: None,
        reset_at_epoch: None,
        quota_state: UsageQuotaStateV1::Available,
        pace_label: None,
        runs_out_label: None,
    }
}

struct FixtureDirectory(Option<PathBuf>);

impl FixtureDirectory {
    fn create() -> Self {
        for _ in 0..128 {
            let sequence = FIXTURE_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = Path::new("/tmp").join(format!("jc-{}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(Some(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create local usage fixture directory: {error}"),
            }
        }
        panic!("find an unused local usage fixture directory");
    }

    fn path(&self) -> &Path {
        self.0.as_deref().expect("fixture directory remains owned")
    }

    fn remove(&mut self) -> std::io::Result<()> {
        if let Some(path) = self.0.as_deref() {
            fs::remove_dir_all(path)?;
        }
        self.0 = None;
        Ok(())
    }
}

impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            drop(fs::remove_dir_all(path));
        }
    }
}

struct LocalUsageFixture {
    directory: FixtureDirectory,
    client: UsageBrokerClient,
    bridge: crate::UsageMenuBarBridge,
    worker: Option<JoinHandle<Result<(), jackin_protocol::usage_broker::UsageCoordinationError>>>,
}

impl LocalUsageFixture {
    fn start(windows: Vec<UsageLimitWindowV1>) -> Self {
        let directory = FixtureDirectory::create();
        seed_projection(directory.path(), windows);
        let resolved_data_dir = fs::canonicalize(directory.path()).expect("canonical fixture root");
        let socket_path = resolved_data_dir.join("usage-broker/run/usage-broker.sock");
        assert!(socket_path.as_os_str().as_encoded_bytes().len() < 104);

        let mut config = UsageBrokerConfig::for_data_dir(directory.path().to_path_buf());
        config.idle_exit = Duration::from_secs(30);
        let client = config.client();
        let worker = jackin_telemetry::spawn::thread_joined_named(
            "count-detail-fixture".to_owned(),
            move || run_usage_monitor_service(config),
        )
        .expect("spawn joined local-only usage service");
        let fixture = Self {
            directory,
            client,
            bridge: crate::UsageMenuBarBridge::create(),
            worker: Some(worker),
        };
        fixture.wait_for_service();
        let root = fixture
            .directory
            .path()
            .to_str()
            .expect("UTF-8 fixture root")
            .to_owned();
        fixture
            .bridge
            .open_runtime(crate::OpenConfig {
                data_dir_override: Some(root.clone()),
                config_root_override: Some(root),
                refresh_floor_secs: 60,
                enabled_surface_ids: vec!["openrouter".to_owned()],
                allow_live_probes: false,
            })
            .expect("open public bridge without live probes");
        fixture
    }

    fn wait_for_service(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if matches!(
                self.client.monitor(MonitorOperation::ServiceStatus),
                Ok(MonitorReply::ServiceStatus { .. })
            ) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "local usage service did not become ready"
            );
            std::thread::park_timeout(Duration::from_millis(10));
        }
    }

    fn stop_and_join(&mut self) -> Result<(), String> {
        let bridge_shutdown = self
            .bridge
            .shutdown()
            .map_err(|error| format!("close public bridge: {error:?}"));
        let service_stop = self
            .client
            .monitor(MonitorOperation::ServiceStop)
            .map_err(|error| format!("stop local usage service: {error:?}"))
            .and_then(|reply| {
                if matches!(reply, MonitorReply::ServiceStopped) {
                    Ok(())
                } else {
                    Err("local usage service returned a non-stop reply".to_owned())
                }
            });
        let joined = self.worker.take().map_or_else(
            || Err("local usage service worker was already joined".to_owned()),
            |worker| match worker.join() {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(format!("local usage service failed: {}", error.message)),
                Err(_) => Err("local usage service worker panicked".to_owned()),
            },
        );
        let removed = self
            .directory
            .remove()
            .map_err(|error| format!("remove local usage fixture directory: {error}"));
        bridge_shutdown?;
        service_stop?;
        joined?;
        removed
    }
}

impl Drop for LocalUsageFixture {
    fn drop(&mut self) {
        drop(self.bridge.shutdown());
        if let Some(worker) = self.worker.take() {
            drop(self.client.monitor(MonitorOperation::ServiceStop));
            drop(worker.join());
        }
    }
}

fn seed_projection(data_dir: &Path, windows: Vec<UsageLimitWindowV1>) {
    let freshness = UsageFreshnessV1 {
        generation: 1,
        phase: UsageFreshnessPhaseV1::Stale,
        last_good_at_epoch: Some(1_780_000_000),
        retry_at_epoch: None,
        is_stale: true,
    };
    let account_id = "count_fixture".to_owned();
    let projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV2,
        projection_id: "count-fixture-projection".to_owned(),
        generated_at_epoch: 1_780_000_000,
        discovery_revision: "count-fixture-discovery".to_owned(),
        broker_instance_id: "count-fixture-instance".to_owned(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: vec![UsageProviderV1 {
            provider_id: "openrouter".to_owned(),
            display_name: "OpenRouter".to_owned(),
            rank: 0,
            membership_state: UsageMembershipStateV1::Current,
            freshness: freshness.clone(),
            accounts: vec![UsageAccountV1 {
                canonical_account_id: account_id,
                identity_kind: UsageIdentityKindV1::ProviderAccountId,
                rank: 0,
                display_label: "Fixture account".to_owned(),
                plan_label: None,
                status_label: None,
                lifecycle: UsageLifecycleV1::Available,
                freshness,
                provenance_count: 1,
                windows,
                metric_groups: Vec::new(),
                credential_expires_at_epoch: None,
                issues: Vec::new(),
            }],
            issues: Vec::new(),
        }],
        unresolved: Vec::new(),
        issues: Vec::new(),
    };
    let envelope = ProjectionStateEnvelope {
        schema_version: ProjectionStateEnvelope::SCHEMA_VERSION,
        projection,
        aliases: Vec::<ProjectionAlias>::new(),
        catalog_revision: "count-fixture-catalog".to_owned(),
        catalog: Vec::new(),
        retry_deadline_epoch: None,
        success_deadline_epoch: None,
        broker_instance_id: "count-fixture-instance".to_owned(),
    };
    FileProjectionStateStore::under_data_dir(data_dir)
        .store(&envelope)
        .expect("seed local public projection store");
}
