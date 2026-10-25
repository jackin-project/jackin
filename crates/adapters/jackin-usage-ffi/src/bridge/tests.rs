// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::io;
use std::process::{Child, ExitStatus, Output};

use jackin_protocol::control::{
    CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit, MeterPercentPoints,
    PercentObservationOrigin, ProviderQuotaState, RawPercentPoints, UsageMeterSide,
};
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageCalendarPeriodV1, UsageFreshnessPhaseV1, UsageFreshnessV1,
    UsageIdentityKindV1, UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1,
    UsageLifecycleV1, UsageLimitWindowV1, UsageMembershipStateV1, UsageMetricGroupKindV1,
    UsageMetricGroupV1, UsageMetricPeriodV1, UsageMetricScopeV1, UsageMetricValueV1,
    UsageProjectionRefreshStateV1, UsageProjectionSchemaV2, UsageProviderV1, UsageQuotaStateV1,
    UsageWindowCategoryV1,
};
use jackin_usage_provider_core::{PercentStyle, ResetStyle, UsageFormatPrefs};

#[test]
fn broker_conflict_has_a_specific_sanitized_bridge_error_code() {
    use jackin_protocol::usage_broker::{UsageCoordinationError, UsageCoordinationErrorKind};

    let error = map_coordination_err(UsageCoordinationError {
        kind: UsageCoordinationErrorKind::BrokerConflict,
        message: "another process owns the usage broker lease".to_owned(),
    });
    assert!(matches!(
        error,
        UsageBridgeError::Rejected { code, .. }
            if code == "coordination_broker_conflict"
    ));
}

#[test]
fn missing_broker_executable_keeps_the_runtime_closed() {
    use jackin_usage::host::UsageBrokerConfig;

    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = UsageMenuBarBridge::create();
    let host_config = to_projection_config(OpenConfig {
        data_dir_override: Some(dir.path().display().to_string()),
        config_root_override: Some(dir.path().join("config").display().to_string()),
        refresh_floor_secs: 120,
        enabled_surface_ids: vec!["claude".to_owned()],
        allow_live_probes: true,
    })
    .expect("host config");
    let mut broker_config = UsageBrokerConfig::for_data_dir(dir.path().to_owned());
    broker_config.service_executable = None;

    let error = bridge
        .open_runtime_with_config(host_config, broker_config)
        .expect_err("activation without an executable must fail");
    assert!(matches!(
        error,
        UsageBridgeError::Rejected { ref code, .. } if code == "coordination_unavailable"
    ));
    assert!(matches!(
        bridge.list_surfaces(),
        Err(UsageBridgeError::RuntimeUnavailable)
    ));
    assert!(bridge.inner.lock().expect("bridge lock").is_none());
}

#[test]
fn local_source_identity_kind_round_trips_in_projection_json() {
    let mut projection = fixture_projection();
    projection.providers[0].accounts[0].identity_kind = UsageIdentityKindV1::LocalSourceHandle;

    let encoded = serde_json::to_string(&projection).expect("projection should encode");
    assert!(encoded.contains("local_source_handle"));
    let decoded: UsageProjectionV1 =
        serde_json::from_str(&encoded).expect("projection should decode");
    assert_eq!(
        decoded.providers[0].accounts[0].identity_kind,
        UsageIdentityKindV1::LocalSourceHandle
    );
}

#[test]
fn broker_socket_alias_binds_reads_and_cleans_under_long_tmpdir() {
    const CHILD_MARKER: &str = "JACKIN_FFI_ALIAS_LONG_TMPDIR_CHILD";
    if std::env::var_os(CHILD_MARKER).is_none() {
        let alias_root = if cfg!(target_os = "macos") {
            std::path::Path::new("/private/tmp")
        } else {
            std::path::Path::new("/tmp")
        };
        let parent = tempfile::tempdir_in(alias_root).expect("TMPDIR fixture parent");
        let long_tmpdir = parent.path().join(format!("tmp-{}", "t".repeat(160)));
        std::fs::create_dir(&long_tmpdir).expect("long TMPDIR fixture");
        let stdout_path = parent.path().join("long-tmpdir-child.stdout");
        let stderr_path = parent.path().join("long-tmpdir-child.stderr");
        let stdout = std::fs::File::create(&stdout_path).expect("child stdout log");
        let stderr = std::fs::File::create(&stderr_path).expect("child stderr log");
        let mut command =
            std::process::Command::new(std::env::current_exe().expect("current test executable"));
        command
            .arg("bridge::tests::broker_socket_alias_binds_reads_and_cleans_under_long_tmpdir")
            .arg("--exact")
            .arg("--nocapture")
            .env("TMPDIR", &long_tmpdir)
            .env(CHILD_MARKER, "1")
            .stdout(std::process::Stdio::from(stdout))
            .stderr(std::process::Stdio::from(stderr));
        let output = run_child_bounded(
            &mut command,
            &stdout_path,
            &stderr_path,
            Duration::from_secs(30),
        )
        .unwrap_or_else(|error| panic!("isolated long-TMPDIR regression failed: {error}"));
        assert!(
            output.status.success(),
            "isolated long-TMPDIR regression failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCoordinationErrorKind};
    use jackin_usage::coordinator::{ProviderProbeOutcome, UsageProviderExecutor};
    use jackin_usage::host::{UsageBrokerConfig, ensure_usage_broker_with_executor};

    struct UnusedExecutor;

    impl UsageProviderExecutor for UnusedExecutor {
        fn probe(
            &self,
            _capability: &UsageAccountCapability,
            _generation: u64,
        ) -> ProviderProbeOutcome {
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::Unavailable,
                message: "long-TMPDIR fixture does not run providers".to_owned(),
                retry_at_epoch: None,
            }
        }
    }

    let injected_tmpdir = std::env::var_os("TMPDIR").expect("isolated TMPDIR");
    assert!(injected_tmpdir.as_encoded_bytes().len() >= 120);
    let data_dir = tempfile::tempdir().expect("tempdir under injected TMPDIR");
    let full_socket = data_dir.path().join("usage-broker/run/usage-broker.sock");
    let socket_limit = if cfg!(target_os = "macos") { 104 } else { 108 };
    assert!(full_socket.as_os_str().as_encoded_bytes().len() >= socket_limit);

    let mut config = UsageBrokerConfig::for_data_dir(data_dir.path().to_owned());
    config.idle_exit = Duration::from_secs(2);
    let client = ensure_usage_broker_with_executor(config, Arc::new(UnusedExecutor))
        .expect("broker must bind through the fixed short alias");
    client
        .current_projection()
        .expect("FFI broker client must connect and read projection");

    let leader_path = data_dir.path().join("usage-broker/run/leader.pid");
    drop(client);
    let deadline = Instant::now() + Duration::from_secs(10);
    while leader_path.exists() && Instant::now() < deadline {
        std::thread::park_timeout(Duration::from_millis(20));
    }
    assert!(
        !leader_path.exists(),
        "broker cleanup must remove lease before fixture exit"
    );
}

fn run_child_bounded(
    command: &mut std::process::Command,
    stdout_path: &std::path::Path,
    stderr_path: &std::path::Path,
    timeout: Duration,
) -> Result<Output, String> {
    let child = command
        .spawn()
        .map_err(|error| format!("spawn failed: {error}"))?;
    let mut child = ChildReaper::new(child);
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                child.mark_reaped();
                break status;
            }
            Ok(None) if Instant::now() >= deadline => {
                child.terminate_and_reap();
                return Err(format!("child exceeded {} seconds", timeout.as_secs()));
            }
            Ok(None) => std::thread::park_timeout(Duration::from_millis(20)),
            Err(error) => {
                child.terminate_and_reap();
                return Err(format!("child status failed: {error}"));
            }
        }
    };
    let stdout =
        std::fs::read(stdout_path).map_err(|error| format!("read child stdout: {error}"))?;
    let stderr =
        std::fs::read(stderr_path).map_err(|error| format!("read child stderr: {error}"))?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

struct ChildReaper {
    child: Option<Child>,
}

impl ChildReaper {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child
            .as_mut()
            .expect("child remains until reaped")
            .try_wait()
    }

    fn mark_reaped(&mut self) {
        self.child.take();
    }

    fn terminate_and_reap(&mut self) {
        if let Some(mut child) = self.child.take() {
            drop(child.kill());
            drop(child.wait());
        }
    }
}

impl Drop for ChildReaper {
    fn drop(&mut self) {
        self.terminate_and_reap();
    }
}

fn open_bridge(dir: &std::path::Path) -> UsageMenuBarBridge {
    open_bridge_with_surfaces(dir, vec!["codex".to_owned(), "claude".to_owned()])
}

fn open_bridge_with_surfaces(
    dir: &std::path::Path,
    enabled_surface_ids: Vec<String>,
) -> UsageMenuBarBridge {
    let bridge = UsageMenuBarBridge::create();
    bridge
        .open_runtime(OpenConfig {
            data_dir_override: Some(dir.display().to_string()),
            config_root_override: Some(dir.join("config").display().to_string()),
            refresh_floor_secs: 120,
            enabled_surface_ids,
            allow_live_probes: false,
        })
        .expect("open offline bridge");
    bridge
}

fn assert_status_bar_export_matches_desktop(bridge: &UsageMenuBarBridge, max: u32) -> Vec<String> {
    let desktop = bridge
        .desktop_projection(max)
        .expect("desktop projection")
        .status_bar_glance_rows;
    let exported = bridge
        .status_bar_provider_glance_rows(max)
        .expect("standalone status-bar rows");
    assert_eq!(exported.len(), desktop.len());
    for (exported, desktop) in exported.iter().zip(&desktop) {
        assert_eq!(exported.surface_id, desktop.surface_id);
        assert_eq!(exported.icon_key, desktop.icon_key);
        assert_eq!(exported.fallback_glyph, desktop.fallback_glyph);
        assert_eq!(exported.usage_url, desktop.usage_url);
        assert_eq!(exported.display_label, desktop.display_label);
        assert_eq!(exported.account_label, desktop.account_label);
        assert_eq!(exported.plan_label, desktop.plan_label);
        assert_eq!(exported.glance_used_percent, desktop.glance_used_percent);
        assert_eq!(
            exported.glance_used_percent_origin,
            desktop.glance_used_percent_origin
        );
        assert_eq!(
            exported.glance_remaining_percent,
            desktop.glance_remaining_percent
        );
        assert_eq!(
            exported.glance_remaining_percent_origin,
            desktop.glance_remaining_percent_origin
        );
        let exported_count = exported.glance_count_quota.as_ref();
        let desktop_count = desktop.glance_count_quota.as_ref();
        assert_eq!(
            exported_count.map(|count| count.used),
            desktop_count.map(|count| count.used)
        );
        assert_eq!(
            exported_count.map(|count| count.limit),
            desktop_count.map(|count| count.limit)
        );
        assert_eq!(
            exported_count.map(|count| count.remaining),
            desktop_count.map(|count| count.remaining)
        );
        assert_eq!(
            exported_count.map(|count| count.unit.as_str()),
            desktop_count.map(|count| count.unit.as_str())
        );
        assert_eq!(
            exported_count.map(|count| count.period.as_str()),
            desktop_count.map(|count| count.period.as_str())
        );
        assert_eq!(
            exported_count.map(|count| count.provenance.as_str()),
            desktop_count.map(|count| count.provenance.as_str())
        );
        assert_eq!(
            exported.glance_provider_quota_state,
            desktop.glance_provider_quota_state
        );
        assert_eq!(exported.glance_resets_at, desktop.glance_resets_at);
        assert_eq!(exported.glance_meter_percent, desktop.glance_meter_percent);
        assert_eq!(exported.glance_meter_side, desktop.glance_meter_side);
        assert_eq!(exported.bar_label, desktop.bar_label);
        assert_eq!(exported.headline, desktop.headline);
        assert_eq!(exported.reset_label, desktop.reset_label);
        assert_eq!(exported.compact_reset_label, desktop.compact_reset_label);
        assert_eq!(exported.exact_reset, desktop.exact_reset);
        assert_eq!(exported.status_word, desktop.status_word);
        assert_eq!(exported.is_refreshing, desktop.is_refreshing);
        assert_eq!(exported.status_label, desktop.status_label);
        assert_eq!(exported.severity, desktop.severity);
        assert_eq!(exported.updated_label, desktop.updated_label);
        assert_eq!(exported.activity_label, desktop.activity_label);
        assert_eq!(exported.activity_kind, desktop.activity_kind);
        assert_eq!(exported.accessibility_label, desktop.accessibility_label);
        assert_eq!(exported.last_error, desktop.last_error);
        assert_eq!(exported.dimmed, desktop.dimmed);
    }
    desktop.into_iter().map(|row| row.surface_id).collect()
}

fn fixture_projection() -> UsageProjectionV1 {
    let fresh = UsageFreshnessV1 {
        generation: 1,
        phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(1_800_000_000),
        retry_at_epoch: None,
        is_stale: false,
    };
    let group = UsageMetricGroupV1 {
        group_id: "weekly-group".to_owned(),
        rank: 0,
        kind: UsageMetricGroupKindV1::Window,
        label: "Weekly quota".to_owned(),
        scope: UsageMetricScopeV1 {
            model: Some("gpt-5".to_owned()),
            ..UsageMetricScopeV1::default()
        },
        observed_at_epoch: Some(1_800_000_000),
        fetched_at_epoch: 1_800_000_001,
        last_success_at_epoch: Some(1_800_000_000),
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::Window {
            remaining_percent: Some(RawPercentPoints::new(64.0).expect("percent")),
            remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
            used_percent: None,
            used_percent_origin: None,
            meter_percent: Some(MeterPercentPoints::new(64.0).expect("meter")),
            meter_side: Some(UsageMeterSide::Remaining),
            count_quota: None,
            provider_quota_state: None,
            money_used_percent: None,
            period: UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Weekly,
            },
            unit: Some("tokens".to_owned()),
        },
        reset_at_epoch: Some(1_800_100_000),
        renews_at_epoch: None,
        issues: Vec::new(),
    };
    let account = |id: &str, rank: u32, label: &str| UsageAccountV1 {
        canonical_account_id: id.to_owned(),
        identity_kind: UsageIdentityKindV1::ProviderAccountId,
        rank,
        display_label: label.to_owned(),
        plan_label: Some("Pro".to_owned()),
        status_label: None,
        lifecycle: UsageLifecycleV1::Available,
        freshness: fresh.clone(),
        provenance_count: 1,
        windows: vec![UsageLimitWindowV1 {
            window_id: format!("{id}-weekly"),
            rank: 0,
            category: UsageWindowCategoryV1::LongRange,
            label: "Weekly".to_owned(),
            value_label: "64% left".to_owned(),
            reset_label: "Resets in 3d".to_owned(),
            remaining_percent: Some(RawPercentPoints::new(64.0).expect("percent")),
            remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
            used_percent: None,
            used_percent_origin: None,
            meter_percent: Some(MeterPercentPoints::new(64.0).expect("meter")),
            meter_side: Some(UsageMeterSide::Remaining),
            count_quota: None,
            provider_quota_state: None,
            money_used_percent: None,
            reset_at_epoch: Some(1_800_100_000),
            quota_state: UsageQuotaStateV1::Available,
            pace_label: None,
            runs_out_label: None,
        }],
        metric_groups: if rank == 0 {
            vec![group.clone()]
        } else {
            Vec::new()
        },
        credential_expires_at_epoch: None,
        issues: Vec::new(),
    };
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV2,
        projection_id: "fixture-projection-1".to_owned(),
        generated_at_epoch: 1_800_000_001,
        discovery_revision: "fixture-catalog-1".to_owned(),
        broker_instance_id: "fixture-broker".to_owned(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: vec![UsageProviderV1 {
            provider_id: "openai".to_owned(),
            display_name: "OpenAI".to_owned(),
            rank: 0,
            membership_state: UsageMembershipStateV1::Current,
            freshness: fresh.clone(),
            accounts: vec![
                account("canonical-openai-a", 0, "alice@example.test"),
                account("canonical-openai-b", 1, "bob@example.test"),
            ],
            issues: Vec::new(),
        }],
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

#[test]
fn desktop_projection_copies_exact_count_and_raw_quota_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    let mut projection = fixture_projection();
    let count = CountQuota::new(
        Some(9_007_199_254_740_993),
        Some(u64::MAX),
        Some(0),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    );
    let (remaining, remaining_origin, used, used_origin, meter, side) = {
        let window = &mut projection.providers[0].accounts[0].windows[0];
        window.value_label = "0 requests left".to_owned();
        window.remaining_percent = Some(RawPercentPoints::new(57.125).expect("raw remaining"));
        window.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
        window.used_percent = Some(RawPercentPoints::new(43.375).expect("raw used"));
        window.used_percent_origin = Some(PercentObservationOrigin::Derived);
        window.count_quota = Some(count);
        window.meter_percent = Some(MeterPercentPoints::new(0.0).expect("count meter"));
        window.meter_side = Some(UsageMeterSide::Remaining);
        (
            window.remaining_percent,
            window.remaining_percent_origin,
            window.used_percent,
            window.used_percent_origin,
            window.meter_percent,
            window.meter_side,
        )
    };
    let group = &mut projection.providers[0].accounts[0].metric_groups[0];
    group.reset_at_epoch = Some(1_800_200_000);
    if let UsageMetricValueV1::Window {
        remaining_percent,
        remaining_percent_origin,
        used_percent,
        used_percent_origin,
        meter_percent,
        meter_side,
        count_quota,
        period,
        unit,
        ..
    } = &mut group.value
    {
        *remaining_percent = remaining;
        *remaining_percent_origin = remaining_origin;
        *used_percent = used;
        *used_percent_origin = used_origin;
        *meter_percent = meter;
        *meter_side = side;
        *count_quota = Some(count);
        *period = UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Daily,
        };
        *unit = Some("requests".to_owned());
    }
    let mut group_only_projection = projection.clone();
    group_only_projection.projection_id = "fixture-projection-2".to_owned();
    group_only_projection.broker_generation += 1;
    group_only_projection.providers[0].accounts[0]
        .windows
        .clear();
    group_only_projection.providers[0].accounts[0].metric_groups[0].reset_at_epoch =
        Some(1_800_200_000);
    projection.providers[0].accounts[0].windows[0].reset_at_epoch = None;
    bridge
        .apply_publication(projection)
        .expect("publish quota fixture");

    let projected = bridge.desktop_projection(3).expect("desktop projection");
    assert_status_bar_export_matches_desktop(&bridge, 3);
    let account = &projected.providers[0].group.accounts[0];
    assert_eq!(account.used_percent, Some(43.375));
    assert_eq!(account.used_percent_origin.as_deref(), Some("derived"));
    assert_eq!(account.remaining_percent, Some(57.125));
    assert_eq!(
        account.remaining_percent_origin.as_deref(),
        Some("provider_reported")
    );
    assert_eq!(
        account.count_quota.as_ref().and_then(|count| count.used),
        count.used
    );
    assert_eq!(
        account.count_quota.as_ref().and_then(|count| count.limit),
        count.limit
    );
    assert_eq!(
        account
            .count_quota
            .as_ref()
            .and_then(|count| count.remaining),
        Some(0)
    );
    assert_eq!(account.resets_at, None);
    assert_eq!(account.provider_quota_state, None);
    assert_eq!(account.meter_percent, Some(0.0));
    assert_eq!(account.meter_side.as_deref(), Some("remaining"));
    assert_eq!(account.headline, "0 requests left");

    let bucket = &projected.providers[0].selected_usage.buckets[0];
    assert_eq!(
        bucket.count_quota.as_ref().and_then(|count| count.used),
        count.used
    );
    assert_eq!(bucket.used_percent, Some(43.375));
    assert_eq!(bucket.remaining_percent, Some(57.125));
    assert_eq!(bucket.provider_quota_state, None);
    assert_eq!(bucket.meter_percent, Some(0.0));
    assert_eq!(bucket.meter_side.as_deref(), Some("remaining"));

    let glance = &projected.glance_rows[0];
    assert_eq!(
        glance
            .glance_count_quota
            .as_ref()
            .and_then(|count| count.used),
        count.used
    );
    assert_eq!(glance.glance_resets_at, None);
    assert_eq!(glance.glance_meter_percent, Some(0.0));
    assert_eq!(glance.glance_meter_side.as_deref(), Some("remaining"));
    assert_eq!(glance.bar_label, "0 requests left");

    bridge
        .apply_publication(group_only_projection)
        .expect("publish group-only quota fixture");
    let inventory = bridge.desktop_inventory().expect("desktop inventory");
    let group_only_account = &inventory
        .groups
        .iter()
        .find(|group| group.surface_id == "codex")
        .expect("Codex account group")
        .accounts[0];
    assert_eq!(group_only_account.used_percent, Some(43.375));
    assert_eq!(group_only_account.remaining_percent, Some(57.125));
    assert_eq!(
        group_only_account
            .count_quota
            .as_ref()
            .and_then(|count| count.used),
        count.used
    );
    assert_eq!(group_only_account.resets_at, Some(1_800_200_000));
    assert_eq!(group_only_account.meter_percent, Some(0.0));
    assert!(group_only_account.headline.contains("0 requests left"));
    bridge.shutdown().expect("shutdown");
}

fn assert_group_and_state_only_unlimited(
    bridge: &UsageMenuBarBridge,
    projection: &UsageProjectionV1,
) {
    let mut group_only_projection = projection.clone();
    group_only_projection.projection_id = "fixture-explicit-unlimited-group-only".to_owned();
    group_only_projection.broker_generation += 1;
    group_only_projection.providers[0].accounts[0]
        .windows
        .clear();
    bridge
        .apply_publication(group_only_projection)
        .expect("publish group-only unlimited quota state");
    let inventory = bridge.desktop_inventory().expect("group-only inventory");
    let group_only_account = &inventory
        .groups
        .iter()
        .find(|group| group.surface_id == "codex")
        .expect("Codex account group")
        .accounts[0];
    assert_eq!(
        group_only_account.provider_quota_state.as_deref(),
        Some("unlimited")
    );
    assert_eq!(group_only_account.remaining_percent, Some(99.0));
    assert_eq!(group_only_account.meter_percent, None);

    let mut state_only_projection = projection.clone();
    state_only_projection.projection_id = "fixture-state-only-unlimited".to_owned();
    state_only_projection.broker_generation += 2;
    {
        let account = &mut state_only_projection.providers[0].accounts[0];
        let window = &mut account.windows[0];
        window.used_percent = None;
        window.used_percent_origin = None;
        window.remaining_percent = None;
        window.remaining_percent_origin = None;
        window.count_quota = None;
        window.money_used_percent = None;
        window.meter_percent = None;
        window.meter_side = None;

        let group = &mut account.metric_groups[0];
        if let UsageMetricValueV1::Window {
            used_percent,
            used_percent_origin,
            remaining_percent,
            remaining_percent_origin,
            count_quota,
            money_used_percent,
            provider_quota_state,
            meter_percent,
            meter_side,
            period,
            unit,
            ..
        } = &mut group.value
        {
            *used_percent = None;
            *used_percent_origin = None;
            *remaining_percent = None;
            *remaining_percent_origin = None;
            *count_quota = None;
            *money_used_percent = None;
            *provider_quota_state = Some(ProviderQuotaState::Unlimited);
            *meter_percent = None;
            *meter_side = None;
            *period = UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Weekly,
            };
            *unit = Some("tokens".to_owned());
        }
        account.windows.clear();
    }
    bridge
        .apply_publication(state_only_projection)
        .expect("publish state-only unlimited quota");
    let state_only = bridge.desktop_projection(3).expect("state-only projection");
    assert_status_bar_export_matches_desktop(bridge, 3);
    let account = &state_only.providers[0].group.accounts[0];
    assert_eq!(account.headline, "Unlimited");
    assert_eq!(account.provider_quota_state.as_deref(), Some("unlimited"));
    assert_eq!(account.meter_percent, None);
    let glance = state_only
        .glance_rows
        .iter()
        .find(|row| row.surface_id == "codex")
        .expect("Codex state-only glance row");
    assert_eq!(glance.headline, "Unlimited");
    assert_eq!(glance.bar_label, "Unlimited");
    let detail = state_only.providers[0]
        .selected_usage
        .detail_presentation
        .rows
        .iter()
        .find(|row| row.row_id == "weekly-group")
        .expect("state-only weekly group detail");
    assert!(detail.display_label.starts_with("Unlimited"));
    assert!(!detail.display_label.contains("0 requests left"));
}

fn publish_exhausted_projection(bridge: &UsageMenuBarBridge, projection: UsageProjectionV1) {
    let mut exhausted_projection = projection;
    exhausted_projection.projection_id = "fixture-explicit-exhausted".to_owned();
    exhausted_projection.broker_generation += 3;
    let exhausted_count = CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    );
    {
        let account = &mut exhausted_projection.providers[0].accounts[0];
        let window = &mut account.windows[0];
        window.value_label = "Exhausted · Reported remaining: 57.125%".to_owned();
        window.remaining_percent = Some(RawPercentPoints::new(57.125).expect("raw remaining"));
        window.count_quota = Some(exhausted_count);
        window.provider_quota_state = Some(ProviderQuotaState::Exhausted);
        window.quota_state = UsageQuotaStateV1::Exhausted;
        window.meter_percent = Some(MeterPercentPoints::new(0.0).expect("exhausted meter"));
        window.meter_side = Some(UsageMeterSide::Remaining);

        let group = &mut account.metric_groups[0];
        group.quota_state = UsageQuotaStateV1::Exhausted;
        if let UsageMetricValueV1::Window {
            remaining_percent,
            remaining_percent_origin,
            count_quota,
            provider_quota_state,
            meter_percent,
            meter_side,
            period,
            unit,
            ..
        } = &mut group.value
        {
            *remaining_percent = window.remaining_percent;
            *remaining_percent_origin = window.remaining_percent_origin;
            *count_quota = window.count_quota;
            *provider_quota_state = window.provider_quota_state;
            *meter_percent = window.meter_percent;
            *meter_side = window.meter_side;
            *period = UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Daily,
            };
            *unit = Some("requests".to_owned());
        }
    }
    bridge
        .apply_publication(exhausted_projection)
        .expect("publish exhausted quota state");
}

fn assert_exhausted_projection(bridge: &UsageMenuBarBridge) {
    let exhausted = bridge.desktop_projection(3).expect("exhausted projection");
    assert_status_bar_export_matches_desktop(bridge, 3);
    let account = &exhausted.providers[0].group.accounts[0];
    assert_eq!(account.provider_quota_state.as_deref(), Some("exhausted"));
    assert_eq!(account.remaining_percent, Some(57.125));
    assert_eq!(
        account.remaining_percent_origin.as_deref(),
        Some("provider_reported")
    );
    assert_eq!(
        account.count_quota.as_ref().and_then(|count| count.used),
        Some(9)
    );
    assert_eq!(
        account.count_quota.as_ref().and_then(|count| count.limit),
        Some(10)
    );
    assert_eq!(
        account
            .count_quota
            .as_ref()
            .and_then(|count| count.remaining),
        Some(1)
    );
    assert_eq!(account.meter_percent, Some(0.0));
    assert_eq!(account.meter_side.as_deref(), Some("remaining"));
    assert!(account.headline.starts_with("Exhausted"));
    assert!(!account.headline.contains("1 requests left"));
    let bucket = &exhausted.providers[0].selected_usage.buckets[0];
    assert_eq!(bucket.provider_quota_state.as_deref(), Some("exhausted"));
    assert_eq!(bucket.remaining_percent, Some(57.125));
    assert_eq!(bucket.meter_percent, Some(0.0));
    assert_eq!(bucket.meter_side.as_deref(), Some("remaining"));
    let glance = exhausted
        .glance_rows
        .iter()
        .find(|row| row.surface_id == "codex")
        .expect("Codex glance row");
    assert!(glance.headline.starts_with("Exhausted"));
    assert!(glance.bar_label.starts_with("Exhausted"));
    assert!(!glance.bar_label.contains("1 requests left"));
    assert_eq!(
        glance.glance_provider_quota_state.as_deref(),
        Some("exhausted")
    );
    assert_eq!(glance.glance_remaining_percent, Some(57.125));
    assert_eq!(
        glance
            .glance_count_quota
            .as_ref()
            .and_then(|count| count.used),
        Some(9)
    );
    assert_eq!(
        glance
            .glance_count_quota
            .as_ref()
            .and_then(|count| count.limit),
        Some(10)
    );
    assert_eq!(
        glance
            .glance_count_quota
            .as_ref()
            .and_then(|count| count.remaining),
        Some(1)
    );
    assert_eq!(glance.glance_meter_percent, Some(0.0));
    assert_eq!(glance.glance_meter_side.as_deref(), Some("remaining"));
    let detail = exhausted.providers[0]
        .selected_usage
        .detail_presentation
        .rows
        .iter()
        .find(|row| row.row_id == "weekly-group")
        .expect("weekly group detail");
    assert!(detail.display_label.starts_with("Exhausted"));
    assert!(!detail.display_label.contains("1 requests left"));
}

#[test]
fn desktop_projection_preserves_explicit_provider_state_and_independent_quota_values() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    let mut projection = fixture_projection();
    let unlimited_count = CountQuota::new(
        Some(0),
        Some(10),
        Some(0),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    );
    {
        let account = &mut projection.providers[0].accounts[0];
        let window = &mut account.windows[0];
        window.value_label = "Unlimited · Reported remaining: 99%".to_owned();
        window.remaining_percent = Some(RawPercentPoints::new(99.0).expect("raw remaining"));
        window.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
        window.count_quota = Some(unlimited_count);
        window.provider_quota_state = Some(ProviderQuotaState::Unlimited);
        window.quota_state = UsageQuotaStateV1::NotApplicable;
        window.meter_percent = None;
        window.meter_side = None;

        let group = &mut account.metric_groups[0];
        group.quota_state = UsageQuotaStateV1::NotApplicable;
        if let UsageMetricValueV1::Window {
            remaining_percent,
            remaining_percent_origin,
            count_quota,
            provider_quota_state,
            meter_percent,
            meter_side,
            period,
            unit,
            ..
        } = &mut group.value
        {
            *remaining_percent = window.remaining_percent;
            *remaining_percent_origin = window.remaining_percent_origin;
            *count_quota = window.count_quota;
            *provider_quota_state = window.provider_quota_state;
            *meter_percent = None;
            *meter_side = None;
            *period = UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Daily,
            };
            *unit = Some("requests".to_owned());
        }
    }
    bridge
        .apply_publication(projection.clone())
        .expect("publish unlimited quota state");
    let unlimited = bridge.desktop_projection(3).expect("unlimited projection");
    assert_status_bar_export_matches_desktop(&bridge, 3);
    let account = &unlimited.providers[0].group.accounts[0];
    assert_eq!(account.provider_quota_state.as_deref(), Some("unlimited"));
    assert_eq!(account.remaining_percent, Some(99.0));
    assert_eq!(
        account.remaining_percent_origin.as_deref(),
        Some("provider_reported")
    );
    assert_eq!(
        account
            .count_quota
            .as_ref()
            .and_then(|count| count.remaining),
        Some(0)
    );
    assert_eq!(account.meter_percent, None);
    assert_eq!(account.meter_side, None);
    assert!(account.headline.starts_with("Unlimited"));
    assert!(!account.headline.contains("0 requests left"));
    let bucket = &unlimited.providers[0].selected_usage.buckets[0];
    assert_eq!(bucket.provider_quota_state.as_deref(), Some("unlimited"));
    assert_eq!(bucket.remaining_percent, Some(99.0));
    assert_eq!(bucket.meter_percent, None);
    assert_eq!(bucket.meter_side, None);
    let glance = unlimited
        .glance_rows
        .iter()
        .find(|row| row.surface_id == "codex")
        .expect("Codex glance row");
    assert!(glance.headline.starts_with("Unlimited"));
    assert!(glance.bar_label.starts_with("Unlimited"));
    assert_eq!(
        glance.glance_provider_quota_state.as_deref(),
        Some("unlimited")
    );
    assert_eq!(glance.glance_remaining_percent, Some(99.0));
    assert_eq!(
        glance
            .glance_count_quota
            .as_ref()
            .and_then(|count| count.remaining),
        Some(0)
    );
    assert_eq!(glance.glance_meter_percent, None);
    assert_eq!(glance.glance_meter_side, None);
    let detail = unlimited.providers[0]
        .selected_usage
        .detail_presentation
        .rows
        .iter()
        .find(|row| row.row_id == "weekly-group")
        .expect("weekly group detail");
    assert!(detail.display_label.starts_with("Unlimited"));

    assert_group_and_state_only_unlimited(&bridge, &projection);

    publish_exhausted_projection(&bridge, projection);
    assert_exhausted_projection(&bridge);
    bridge.shutdown().expect("shutdown");
}

#[test]
fn desktop_projection_preserves_raw_remaining_with_complete_and_incomplete_requests_counts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    let mut projection = fixture_projection();
    let count = CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    );
    let account = &mut projection.providers[0].accounts[0];
    let (remaining, remaining_origin, meter, side) = {
        let window = &mut account.windows[0];
        window.value_label = "1 requests left".to_owned();
        window.remaining_percent = Some(RawPercentPoints::new(57.125).expect("raw remaining"));
        window.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
        window.count_quota = Some(count);
        window.meter_percent = Some(MeterPercentPoints::new(10.0).expect("count meter"));
        window.meter_side = Some(UsageMeterSide::Remaining);
        (
            window.remaining_percent,
            window.remaining_percent_origin,
            window.meter_percent,
            window.meter_side,
        )
    };
    let group = &mut account.metric_groups[0];
    if let UsageMetricValueV1::Window {
        remaining_percent,
        remaining_percent_origin,
        count_quota,
        meter_percent,
        meter_side,
        period,
        unit,
        ..
    } = &mut group.value
    {
        *remaining_percent = remaining;
        *remaining_percent_origin = remaining_origin;
        *count_quota = Some(count);
        *meter_percent = meter;
        *meter_side = side;
        *period = UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Daily,
        };
        *unit = Some("requests".to_owned());
    }
    bridge
        .apply_publication(projection.clone())
        .expect("complete count fixture");
    let projected = bridge.desktop_projection(3).expect("complete projection");
    assert_status_bar_export_matches_desktop(&bridge, 3);
    let account = &projected.providers[0].group.accounts[0];
    assert_eq!(account.remaining_percent, Some(57.125));
    assert_eq!(
        account.remaining_percent_origin.as_deref(),
        Some("provider_reported")
    );
    assert_eq!(
        account.count_quota.as_ref().and_then(|quota| quota.used),
        Some(9)
    );
    assert_eq!(
        account.count_quota.as_ref().and_then(|quota| quota.limit),
        Some(10)
    );
    assert_eq!(
        account
            .count_quota
            .as_ref()
            .and_then(|quota| quota.remaining),
        Some(1)
    );
    assert_eq!(account.meter_percent, Some(10.0));
    assert_eq!(account.meter_side.as_deref(), Some("remaining"));
    assert_eq!(account.headline, "1 requests left");
    let bucket = &projected.providers[0].selected_usage.buckets[0];
    assert_eq!(bucket.remaining_percent, Some(57.125));
    assert_eq!(bucket.meter_percent, Some(10.0));
    assert_eq!(bucket.meter_side.as_deref(), Some("remaining"));
    let glance = &projected.glance_rows[0];
    assert_eq!(glance.glance_remaining_percent, Some(57.125));
    assert_eq!(glance.glance_meter_percent, Some(10.0));
    assert_eq!(glance.bar_label, "1 requests left");

    let mut incomplete = projection;
    incomplete.projection_id = "fixture-projection-incomplete-count".to_owned();
    incomplete.broker_generation += 1;
    let account = &mut incomplete.providers[0].accounts[0];
    let count = CountQuota::new(
        Some(9),
        None,
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    );
    account.windows[0].count_quota = Some(count);
    account.windows[0].meter_percent = None;
    account.windows[0].meter_side = None;
    let group = &mut account.metric_groups[0];
    if let UsageMetricValueV1::Window {
        count_quota,
        meter_percent,
        meter_side,
        ..
    } = &mut group.value
    {
        *count_quota = Some(count);
        *meter_percent = None;
        *meter_side = None;
    }
    bridge
        .apply_publication(incomplete)
        .expect("incomplete count fixture");
    let projected = bridge.desktop_projection(3).expect("incomplete projection");
    assert!(assert_status_bar_export_matches_desktop(&bridge, 3).is_empty());
    let account = &projected.providers[0].group.accounts[0];
    assert_eq!(account.remaining_percent, Some(57.125));
    assert_eq!(account.meter_percent, None);
    assert_eq!(account.meter_side, None);
    assert_eq!(
        account.count_quota.as_ref().and_then(|quota| quota.limit),
        None
    );
    assert_eq!(account.headline, "1 requests left");
    let glance = &projected.glance_rows[0];
    assert_eq!(glance.glance_remaining_percent, Some(57.125));
    assert_eq!(glance.glance_meter_percent, None);
    assert_eq!(glance.glance_meter_side, None);
    bridge.shutdown().expect("shutdown");
}

#[test]
fn status_bar_excludes_unknown_quota_without_numeric_observations() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    let mut projection = fixture_projection();
    let account = &mut projection.providers[0].accounts[0];
    let window = &mut account.windows[0];
    window.used_percent = None;
    window.used_percent_origin = None;
    window.remaining_percent = None;
    window.remaining_percent_origin = None;
    window.count_quota = None;
    window.provider_quota_state = None;
    window.money_used_percent = None;
    window.meter_percent = None;
    window.meter_side = None;

    let group = &mut account.metric_groups[0];
    group.quota_state = UsageQuotaStateV1::Unknown;
    if let UsageMetricValueV1::Window {
        used_percent,
        used_percent_origin,
        remaining_percent,
        remaining_percent_origin,
        count_quota,
        provider_quota_state,
        money_used_percent,
        meter_percent,
        meter_side,
        ..
    } = &mut group.value
    {
        *used_percent = None;
        *used_percent_origin = None;
        *remaining_percent = None;
        *remaining_percent_origin = None;
        *count_quota = None;
        *provider_quota_state = None;
        *money_used_percent = None;
        *meter_percent = None;
        *meter_side = None;
    }
    bridge
        .apply_publication(projection)
        .expect("publish unknown quota state");

    let desktop = bridge.desktop_projection(3).expect("desktop projection");
    let row = desktop
        .glance_rows
        .iter()
        .find(|row| row.surface_id == "codex")
        .expect("Codex detail row remains visible");
    assert_eq!(row.glance_provider_quota_state, None);
    assert_eq!(row.glance_used_percent, None);
    assert_eq!(row.glance_remaining_percent, None);
    assert!(row.glance_count_quota.is_none());
    assert_eq!(row.glance_meter_percent, None);
    assert_eq!(row.glance_meter_side, None);
    assert!(desktop.status_bar_glance_rows.is_empty());
    assert!(assert_status_bar_export_matches_desktop(&bridge, 3).is_empty());
    bridge.shutdown().expect("shutdown");
}

fn configure_runtime_selected_metric_groups(
    bridge: &UsageMenuBarBridge,
    projection: &mut UsageProjectionV1,
) {
    let codex_account = &mut projection.providers[0].accounts[0];
    let mut codex_session = codex_account.windows[0].clone();
    codex_session.window_id = "codex-session".to_owned();
    codex_session.category = UsageWindowCategoryV1::Session;
    codex_session.label = "Session".to_owned();
    codex_session.value_label = "90% left".to_owned();
    codex_session.remaining_percent = Some(RawPercentPoints::new(90.0).expect("session raw"));
    codex_session.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    codex_session.meter_percent = None;
    codex_session.meter_side = None;
    codex_session.provider_quota_state = Some(ProviderQuotaState::Unlimited);
    codex_session.quota_state = UsageQuotaStateV1::NotApplicable;
    codex_session.count_quota = Some(CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    codex_session.reset_at_epoch = Some(1_800_300_000);
    codex_account.windows[0] = codex_session;
    codex_account.issues.push(UsageIssueV1 {
        code: "account-fallback".to_owned(),
        scope: UsageIssueScopeV1::Account,
        recoverability: UsageIssueRecoverabilityV1::Retryable,
        message: "account fallback must not replace selected group".to_owned(),
        retry_at_epoch: None,
    });
    let codex_weekly = &mut codex_account.metric_groups[0];
    codex_weekly.reset_at_epoch = None;
    codex_weekly.last_success_at_epoch = None;
    codex_weekly.phase = UsageFreshnessPhaseV1::Failed;
    codex_weekly.is_stale = false;
    codex_weekly.quota_state = UsageQuotaStateV1::Error;
    codex_weekly.issues.clear();
    if let UsageMetricValueV1::Window {
        used_percent,
        used_percent_origin,
        remaining_percent,
        remaining_percent_origin,
        count_quota,
        provider_quota_state,
        money_used_percent,
        meter_percent,
        meter_side,
        period,
        ..
    } = &mut codex_weekly.value
    {
        *used_percent = Some(RawPercentPoints::new(43.375).expect("weekly used"));
        *used_percent_origin = Some(PercentObservationOrigin::Derived);
        *remaining_percent = Some(RawPercentPoints::new(57.125).expect("weekly remaining"));
        *remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
        *count_quota = None;
        *provider_quota_state = None;
        *money_used_percent = None;
        *meter_percent = Some(MeterPercentPoints::new(0.0).expect("weekly meter"));
        *meter_side = Some(UsageMeterSide::Remaining);
        *period = UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Weekly,
        };
    }

    let mut amp_provider = projection.providers[0].clone();
    amp_provider.provider_id = "amp".to_owned();
    amp_provider.display_name = "Amp".to_owned();
    amp_provider.rank = 1;
    amp_provider.accounts.truncate(1);
    let amp_account = &mut amp_provider.accounts[0];
    amp_account.canonical_account_id = "canonical-amp-a".to_owned();
    amp_account.display_label = "amp@example.test".to_owned();
    amp_account.issues.clear();
    let mut amp_session = amp_account.windows[0].clone();
    amp_session.window_id = "amp-session".to_owned();
    amp_session.category = UsageWindowCategoryV1::Session;
    amp_session.label = "Session".to_owned();
    amp_session.value_label = "88% left".to_owned();
    amp_session.remaining_percent = Some(RawPercentPoints::new(88.0).expect("session raw"));
    amp_session.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    amp_session.provider_quota_state = None;
    amp_session.meter_percent = Some(MeterPercentPoints::new(88.0).expect("session meter"));
    amp_session.meter_side = Some(UsageMeterSide::Remaining);
    amp_session.reset_at_epoch = Some(1_800_400_000);
    amp_account.windows = vec![amp_session];
    let mut amp_weekly = amp_account.metric_groups[0].clone();
    amp_weekly.group_id = "amp-weekly".to_owned();
    amp_weekly.rank = 0;
    amp_weekly.reset_at_epoch = Some(1_800_500_000);
    let mut amp_daily = amp_weekly.clone();
    amp_daily.group_id = "amp-daily".to_owned();
    amp_daily.rank = 1;
    amp_daily.label = "Daily quota".to_owned();
    amp_daily.scope.model = None;
    amp_daily.reset_at_epoch = None;
    amp_daily.last_success_at_epoch = None;
    amp_daily.phase = UsageFreshnessPhaseV1::Stale;
    amp_daily.is_stale = true;
    amp_daily.quota_state = UsageQuotaStateV1::Warning;
    amp_daily.issues = vec![UsageIssueV1 {
        code: "daily-stale".to_owned(),
        scope: UsageIssueScopeV1::Group,
        recoverability: UsageIssueRecoverabilityV1::Retryable,
        message: "selected daily group is stale".to_owned(),
        retry_at_epoch: None,
    }];
    if let UsageMetricValueV1::Window {
        used_percent,
        used_percent_origin,
        remaining_percent,
        remaining_percent_origin,
        count_quota,
        provider_quota_state,
        money_used_percent,
        meter_percent,
        meter_side,
        period,
        ..
    } = &mut amp_daily.value
    {
        *used_percent = Some(RawPercentPoints::new(12.345).expect("daily used"));
        *used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
        *remaining_percent = None;
        *remaining_percent_origin = None;
        *count_quota = None;
        *provider_quota_state = None;
        *money_used_percent = None;
        *meter_percent = Some(MeterPercentPoints::new(12.25).expect("daily meter"));
        *meter_side = Some(UsageMeterSide::Used);
        *period = UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Daily,
        };
    }
    amp_account.metric_groups = vec![amp_weekly, amp_daily];
    projection.providers.push(amp_provider);
    bridge
        .apply_publication(projection.clone())
        .expect("publish runtime-selected metric groups");
}

fn assert_runtime_selected_metric_glances(projected: &DesktopProjectionDto) {
    let codex_account = &projected.providers[0].group.accounts[0];
    assert_eq!(
        codex_account.provider_quota_state.as_deref(),
        Some("unlimited")
    );
    assert!(codex_account.headline.starts_with("Unlimited"));
    let codex = projected
        .glance_rows
        .iter()
        .find(|row| row.surface_id == "codex")
        .expect("Codex glance row");
    assert_eq!(codex.glance_used_percent, Some(43.375));
    assert_eq!(codex.glance_used_percent_origin.as_deref(), Some("derived"));
    assert_eq!(codex.glance_remaining_percent, Some(57.125));
    assert_eq!(codex.glance_provider_quota_state, None);
    assert_eq!(
        codex.glance_remaining_percent_origin.as_deref(),
        Some("provider_reported")
    );
    assert!(codex.glance_count_quota.is_none());
    assert_eq!(codex.glance_meter_percent, Some(0.0));
    assert_eq!(codex.glance_meter_side.as_deref(), Some("remaining"));
    assert_eq!(codex.glance_resets_at, None);
    assert_eq!(codex.reset_label, None);
    assert_eq!(codex.exact_reset, None);
    assert_eq!(codex.status_word, "error");
    assert_eq!(codex.last_error, None);
    assert_eq!(codex.activity_label, "error");
    assert_eq!(codex.updated_label, "No successful update");
    assert_eq!(codex.headline, "gpt-5 43.38% used");
    assert_eq!(codex.bar_label, "43.38% used");

    let amp = projected
        .glance_rows
        .iter()
        .find(|row| row.surface_id == "amp")
        .expect("Amp glance row");
    assert_eq!(amp.glance_used_percent, Some(12.345));
    assert_eq!(
        amp.glance_used_percent_origin.as_deref(),
        Some("provider_reported")
    );
    assert_eq!(amp.glance_remaining_percent, None);
    assert_eq!(amp.glance_meter_percent, Some(12.25));
    assert_eq!(amp.glance_meter_side.as_deref(), Some("used"));
    assert_eq!(amp.glance_resets_at, None);
    assert_eq!(amp.reset_label, None);
    assert_eq!(amp.exact_reset, None);
    assert_eq!(amp.status_word, "stale");
    assert_eq!(amp.severity, "warn");
    assert_eq!(
        amp.last_error.as_deref(),
        Some("selected daily group is stale")
    );
    assert_eq!(amp.activity_label, "selected daily group is stale");
    assert_eq!(amp.updated_label, "No successful update");
    assert_eq!(amp.headline, "12.35% used");
    assert_eq!(amp.bar_label, "12.35% used");
}

fn publish_absent_selected_values(bridge: &UsageMenuBarBridge, projection: UsageProjectionV1) {
    let mut absent_selected_values = projection;
    absent_selected_values.projection_id = "fixture-selected-glance-values-absent".to_owned();
    absent_selected_values.broker_generation += 1;
    {
        let account = &mut absent_selected_values.providers[0].accounts[0];
        let selected = &mut account.metric_groups[0];
        selected.issues = vec![UsageIssueV1 {
            code: "weekly-failed".to_owned(),
            scope: UsageIssueScopeV1::Group,
            recoverability: UsageIssueRecoverabilityV1::Retryable,
            message: "selected weekly group failed".to_owned(),
            retry_at_epoch: None,
        }];
        if let UsageMetricValueV1::Window {
            used_percent,
            used_percent_origin,
            remaining_percent,
            remaining_percent_origin,
            count_quota,
            provider_quota_state,
            money_used_percent,
            meter_percent,
            meter_side,
            ..
        } = &mut selected.value
        {
            *used_percent = None;
            *used_percent_origin = None;
            *remaining_percent = None;
            *remaining_percent_origin = None;
            *count_quota = None;
            *provider_quota_state = None;
            *money_used_percent = None;
            *meter_percent = None;
            *meter_side = None;
        }
    }
    {
        let account = &mut absent_selected_values.providers[1].accounts[0];
        account.windows[0].provider_quota_state = Some(ProviderQuotaState::Unlimited);
        account.windows[0].meter_percent = None;
        account.windows[0].meter_side = None;
        let selected = &mut account.metric_groups[1];
        selected.reset_at_epoch = Some(1_800_600_000);
        if let UsageMetricValueV1::Window {
            used_percent,
            used_percent_origin,
            remaining_percent,
            remaining_percent_origin,
            count_quota,
            provider_quota_state,
            money_used_percent,
            meter_percent,
            meter_side,
            ..
        } = &mut selected.value
        {
            *used_percent = None;
            *used_percent_origin = None;
            *remaining_percent = None;
            *remaining_percent_origin = None;
            *count_quota = None;
            *provider_quota_state = None;
            *money_used_percent = None;
            *meter_percent = None;
            *meter_side = None;
        }
    }
    bridge
        .apply_publication(absent_selected_values)
        .expect("publish selected groups without quota observations");
}

fn assert_absent_selected_glances(absent: &DesktopProjectionDto) {
    let codex = absent
        .glance_rows
        .iter()
        .find(|row| row.surface_id == "codex")
        .expect("Codex glance row");
    assert_eq!(codex.glance_used_percent, None);
    assert_eq!(codex.glance_used_percent_origin, None);
    assert_eq!(codex.glance_remaining_percent, None);
    assert_eq!(codex.glance_remaining_percent_origin, None);
    assert!(codex.glance_count_quota.is_none());
    assert_eq!(codex.glance_provider_quota_state, None);
    assert_eq!(codex.glance_meter_percent, None);
    assert_eq!(codex.glance_meter_side, None);
    assert_eq!(codex.glance_resets_at, None);
    assert_eq!(codex.reset_label, None);
    assert_eq!(codex.exact_reset, None);
    assert_eq!(codex.status_word, "error");
    assert_eq!(
        codex.last_error.as_deref(),
        Some("selected weekly group failed")
    );
    assert_eq!(codex.activity_label, "selected weekly group failed");
    assert_eq!(codex.updated_label, "No successful update");
    assert_eq!(codex.headline, "weekly · quota error");
    assert_eq!(codex.bar_label, "weekly · quota error");
    assert!(codex.accessibility_label.contains("weekly · quota error"));
    assert!(!codex.accessibility_label.contains("Unlimited"));
    assert!(!codex.accessibility_label.contains("90%"));

    let amp = absent
        .glance_rows
        .iter()
        .find(|row| row.surface_id == "amp")
        .expect("Amp glance row");
    assert_eq!(amp.glance_used_percent, None);
    assert_eq!(amp.glance_used_percent_origin, None);
    assert_eq!(amp.glance_remaining_percent, None);
    assert_eq!(amp.glance_remaining_percent_origin, None);
    assert!(amp.glance_count_quota.is_none());
    assert_eq!(amp.glance_provider_quota_state, None);
    assert_eq!(amp.glance_meter_percent, None);
    assert_eq!(amp.glance_meter_side, None);
    assert_eq!(amp.glance_resets_at, Some(1_800_600_000));
    assert!(amp.reset_label.is_some());
    assert!(amp.exact_reset.is_some());
    assert_eq!(amp.status_word, "stale");
    assert_eq!(
        amp.last_error.as_deref(),
        Some("selected daily group is stale")
    );
    assert_eq!(amp.activity_label, "selected daily group is stale");
    assert_eq!(amp.updated_label, "No successful update");
    assert_eq!(amp.headline, "daily · quota warning");
    assert_eq!(amp.bar_label, "daily · quota warning");
    assert!(amp.accessibility_label.contains("daily · quota warning"));
    assert!(!amp.accessibility_label.contains("Unlimited"));
    assert!(!amp.accessibility_label.contains("88%"));
}

#[test]
fn desktop_glance_uses_runtime_selected_metric_group_as_an_atomic_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge_with_surfaces(dir.path(), vec!["codex".to_owned(), "amp".to_owned()]);
    let mut projection = fixture_projection();

    configure_runtime_selected_metric_groups(&bridge, &mut projection);

    let projected = bridge.desktop_projection(3).expect("desktop projection");
    assert_runtime_selected_metric_glances(&projected);
    assert_status_bar_export_matches_desktop(&bridge, 3);

    publish_absent_selected_values(&bridge, projection);

    let absent = bridge
        .desktop_projection(3)
        .expect("selected groups without quota observations");
    assert_absent_selected_glances(&absent);
    assert!(assert_status_bar_export_matches_desktop(&bridge, 3).is_empty());
    bridge.shutdown().expect("shutdown");
}

fn assert_percent_and_money_collision_sources(
    provider: &UsageProviderV1,
    account: &mut UsageAccountV1,
    surface: HostSurfaceId,
) {
    let collision_cases = [
        (
            Some(43.375),
            Some(PercentObservationOrigin::Derived),
            None,
            None,
            -10.0,
            43.375,
            UsageMeterSide::Used,
            "Derived used: 43.38% · Money ratio: -10% used",
        ),
        (
            None,
            None,
            Some(57.125),
            Some(PercentObservationOrigin::ProviderReported),
            150.0,
            57.125,
            UsageMeterSide::Remaining,
            "Reported remaining: 57.13% · Money ratio: 150% used",
        ),
    ];
    for percent_style in [PercentStyle::Left, PercentStyle::Used] {
        for (used, used_origin, remaining, remaining_origin, money, meter, side, expected) in
            collision_cases
        {
            let UsageMetricValueV1::Window {
                remaining_percent,
                remaining_percent_origin,
                used_percent,
                used_percent_origin,
                count_quota,
                money_used_percent,
                meter_percent,
                meter_side,
                ..
            } = &mut account.metric_groups[0].value
            else {
                panic!("fixture group is a Window");
            };
            *remaining_percent =
                remaining.map(|value| RawPercentPoints::new(value).expect("remaining source"));
            *remaining_percent_origin = remaining_origin;
            *used_percent = used.map(|value| RawPercentPoints::new(value).expect("used source"));
            *used_percent_origin = used_origin;
            *count_quota = None;
            *money_used_percent = Some(RawPercentPoints::new(money).expect("money ratio"));
            *meter_percent = Some(MeterPercentPoints::new(meter).expect("meter"));
            *meter_side = Some(side);

            let prefs = UsageFormatPrefs {
                percent_style,
                reset_style: ResetStyle::Countdown,
            };
            let account_dto =
                crate::presentation::account_dto(surface, provider, account, prefs, true);
            let glance = crate::presentation::glance_row(
                surface,
                Some(provider),
                account,
                Some(&account.metric_groups[0]),
                prefs,
            );
            let detail = crate::presentation::view_dto(surface, account, prefs);
            assert_eq!(account_dto.headline, expected);
            assert!(account_dto.remaining_label.contains(expected));
            assert_eq!(glance.headline, expected);
            assert_eq!(glance.bar_label, expected);
            assert_eq!(account_dto.used_percent, used);
            assert_eq!(account_dto.remaining_percent, remaining);
            assert_eq!(
                account_dto.used_percent_origin.as_deref(),
                used_origin.map(|value| {
                    match value {
                        PercentObservationOrigin::ProviderReported => "provider_reported",
                        PercentObservationOrigin::Derived => "derived",
                        PercentObservationOrigin::HistoricalUnknown => "historical_unknown",
                    }
                })
            );
            assert_eq!(
                account_dto.remaining_percent_origin.as_deref(),
                remaining_origin.map(|value| match value {
                    PercentObservationOrigin::ProviderReported => "provider_reported",
                    PercentObservationOrigin::Derived => "derived",
                    PercentObservationOrigin::HistoricalUnknown => "historical_unknown",
                })
            );
            assert_eq!(account_dto.meter_percent, Some(meter));
            assert_eq!(account_dto.meter_side.as_deref(), Some(side.wire_label()));
            let detail_group = detail
                .detail_presentation
                .rows
                .iter()
                .find(|row| row.row_id == "weekly-group")
                .expect("Window detail row");
            assert!(detail_group.display_label.contains(expected));
        }
    }
}

#[test]
fn metric_only_account_glance_and_detail_labels_preserve_used_and_signed_money_sources() {
    let projection = fixture_projection();
    let provider = &projection.providers[0];
    let mut account = provider.accounts[0].clone();
    account.windows.clear();
    account.metric_groups[0].scope.model = None;
    let surface = HostSurfaceId::from_id("codex").expect("Codex surface");

    {
        let UsageMetricValueV1::Window {
            remaining_percent,
            remaining_percent_origin,
            used_percent,
            used_percent_origin,
            count_quota,
            money_used_percent,
            meter_percent,
            meter_side,
            ..
        } = &mut account.metric_groups[0].value
        else {
            panic!("fixture group is a Window");
        };
        *remaining_percent = None;
        *remaining_percent_origin = None;
        *used_percent = Some(RawPercentPoints::new(43.375).expect("used source"));
        *used_percent_origin = Some(PercentObservationOrigin::Derived);
        *count_quota = None;
        *money_used_percent = None;
        *meter_percent = Some(MeterPercentPoints::new(43.375).expect("used meter"));
        *meter_side = Some(UsageMeterSide::Used);
    }

    for percent_style in [PercentStyle::Left, PercentStyle::Used] {
        let prefs = UsageFormatPrefs {
            percent_style,
            reset_style: ResetStyle::Countdown,
        };
        let account_dto =
            crate::presentation::account_dto(surface, provider, &account, prefs, true);
        let glance = crate::presentation::glance_row(
            surface,
            Some(provider),
            &account,
            Some(&account.metric_groups[0]),
            prefs,
        );
        let detail = crate::presentation::view_dto(surface, &account, prefs);
        assert_eq!(account_dto.headline, "43.38% used");
        assert_eq!(glance.headline, "43.38% used");
        assert_eq!(
            glance.bar_label,
            match percent_style {
                PercentStyle::Left => "43.38% used",
                PercentStyle::Used => "43.38%",
            }
        );
        assert_eq!(account_dto.used_percent, Some(43.375));
        assert_eq!(account_dto.remaining_percent, None);
        let detail_group = detail
            .detail_presentation
            .rows
            .iter()
            .find(|row| row.row_id == "weekly-group")
            .expect("Window detail row");
        assert!(detail_group.display_label.contains("43.38% used"));
    }

    assert_percent_and_money_collision_sources(provider, &mut account, surface);

    for raw_money_ratio in [-10.0, 150.0] {
        let money = RawPercentPoints::new(raw_money_ratio).expect("signed money ratio");
        let UsageMetricValueV1::Window {
            remaining_percent,
            remaining_percent_origin,
            used_percent,
            used_percent_origin,
            count_quota,
            money_used_percent,
            meter_percent,
            meter_side,
            ..
        } = &mut account.metric_groups[0].value
        else {
            panic!("fixture group is a Window");
        };
        *remaining_percent = None;
        *remaining_percent_origin = None;
        *used_percent = None;
        *used_percent_origin = None;
        *count_quota = None;
        *money_used_percent = Some(money);
        *meter_percent =
            Some(MeterPercentPoints::new(raw_money_ratio.clamp(0.0, 100.0)).expect("meter"));
        *meter_side = Some(UsageMeterSide::Used);
        let prefs = UsageFormatPrefs::default();
        let account_dto =
            crate::presentation::account_dto(surface, provider, &account, prefs, true);
        let glance = crate::presentation::glance_row(
            surface,
            Some(provider),
            &account,
            Some(&account.metric_groups[0]),
            prefs,
        );
        let detail = crate::presentation::view_dto(surface, &account, prefs);
        let label = format!("{}% used", money.display_value());
        assert_eq!(account_dto.headline, label);
        assert_eq!(glance.headline, label);
        assert_eq!(glance.bar_label, label);
        assert_eq!(account_dto.used_percent, None);
        assert_eq!(account_dto.remaining_percent, None);
        let detail_group = detail
            .detail_presentation
            .rows
            .iter()
            .find(|row| row.row_id == "weekly-group")
            .expect("Window detail row");
        assert!(detail_group.display_label.contains(&label));
    }
}

#[test]
fn metric_only_request_summary_renders_u64_max_as_a_rust_owned_literal() {
    let projection = fixture_projection();
    let provider = &projection.providers[0];
    let mut account = provider.accounts[0].clone();
    account.windows.clear();
    let count = CountQuota::new(
        None,
        Some(u64::MAX),
        Some(u64::MAX),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    );
    let UsageMetricValueV1::Window {
        remaining_percent,
        remaining_percent_origin,
        used_percent,
        used_percent_origin,
        count_quota,
        meter_percent,
        meter_side,
        period,
        unit,
        ..
    } = &mut account.metric_groups[0].value
    else {
        panic!("fixture group is a Window");
    };
    *remaining_percent = None;
    *remaining_percent_origin = None;
    *used_percent = None;
    *used_percent_origin = None;
    *count_quota = Some(count);
    *meter_percent = Some(MeterPercentPoints::new(100.0).expect("count meter"));
    *meter_side = Some(UsageMeterSide::Remaining);
    *period = UsageMetricPeriodV1::Calendar {
        granularity: UsageCalendarPeriodV1::Daily,
    };
    *unit = Some("requests".to_owned());

    let surface = HostSurfaceId::from_id("codex").expect("Codex surface");
    let prefs = UsageFormatPrefs::default();
    let account_dto = crate::presentation::account_dto(surface, provider, &account, prefs, true);
    let glance = crate::presentation::glance_row(
        surface,
        Some(provider),
        &account,
        Some(&account.metric_groups[0]),
        prefs,
    );
    let detail = crate::presentation::view_dto(surface, &account, prefs);
    let literal = "18446744073709551615 requests left";
    let headline = format!("gpt-5 {literal}");
    assert_eq!(account_dto.headline, headline);
    assert_eq!(
        account_dto
            .count_quota
            .as_ref()
            .and_then(|quota| quota.remaining),
        Some(u64::MAX)
    );
    assert_eq!(account_dto.meter_percent, Some(100.0));
    assert_eq!(glance.headline, headline);
    assert_eq!(glance.bar_label, literal);
    let detail_group = detail
        .detail_presentation
        .rows
        .iter()
        .find(|row| row.row_id == "weekly-group")
        .expect("Window detail row");
    assert!(detail_group.display_label.contains(literal));
}

fn provider_with_glance(
    source: &UsageProviderV1,
    provider_id: &str,
    rank: u32,
    reset_at_epoch: Option<i64>,
    remaining: f64,
) -> UsageProviderV1 {
    let mut provider = source.clone();
    provider.provider_id = provider_id.to_owned();
    provider.display_name = provider_id.to_owned();
    provider.rank = rank;
    for account in &mut provider.accounts {
        account.canonical_account_id = format!("{provider_id}-account-{}", account.rank);
        account.display_label = format!("{provider_id}-account-{}@example.test", account.rank);
        for window in &mut account.windows {
            window.window_id = format!("{provider_id}-window-{}", account.rank);
            window.value_label = format!("{remaining}% left");
            window.remaining_percent = Some(RawPercentPoints::new(remaining).expect("percent"));
            window.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
            window.meter_percent =
                Some(MeterPercentPoints::new(remaining.clamp(0.0, 100.0)).expect("meter"));
            window.meter_side = Some(UsageMeterSide::Remaining);
            window.reset_at_epoch = reset_at_epoch;
        }
        for group in &mut account.metric_groups {
            group.group_id = format!("{provider_id}-group-{}", group.rank);
            group.reset_at_epoch = reset_at_epoch;
            if let UsageMetricValueV1::Window {
                remaining_percent,
                remaining_percent_origin,
                meter_percent,
                meter_side,
                ..
            } = &mut group.value
            {
                *remaining_percent = Some(RawPercentPoints::new(remaining).expect("percent"));
                *remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
                *meter_percent =
                    Some(MeterPercentPoints::new(remaining.clamp(0.0, 100.0)).expect("meter"));
                *meter_side = Some(UsageMeterSide::Remaining);
            }
        }
    }
    provider
}

fn fixture_with_provider_glances(specs: &[(&str, u32, Option<i64>, f64)]) -> UsageProjectionV1 {
    let mut projection = fixture_projection();
    let source = projection.providers[0].clone();
    projection.providers = specs
        .iter()
        .map(|(provider_id, rank, reset_at_epoch, remaining)| {
            provider_with_glance(&source, provider_id, *rank, *reset_at_epoch, *remaining)
        })
        .collect();
    projection
}

#[test]
fn desktop_projection_sorts_used_side_by_remaining_capacity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge =
        open_bridge_with_surfaces(dir.path(), vec!["codex".to_owned(), "claude".to_owned()]);
    let mut projection =
        fixture_with_provider_glances(&[("openai", 0, None, 40.0), ("anthropic", 1, None, 70.0)]);
    for account in &mut projection.providers[1].accounts {
        for window in &mut account.windows {
            window.value_label = "70% used".to_owned();
            window.remaining_percent = None;
            window.remaining_percent_origin = None;
            window.used_percent = Some(RawPercentPoints::new(70.0).expect("used"));
            window.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
            window.meter_percent = Some(MeterPercentPoints::new(70.0).expect("meter"));
            window.meter_side = Some(UsageMeterSide::Used);
        }
        for group in &mut account.metric_groups {
            if let UsageMetricValueV1::Window {
                remaining_percent,
                remaining_percent_origin,
                used_percent,
                used_percent_origin,
                meter_percent,
                meter_side,
                ..
            } = &mut group.value
            {
                *remaining_percent = None;
                *remaining_percent_origin = None;
                *used_percent = Some(RawPercentPoints::new(70.0).expect("used"));
                *used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
                *meter_percent = Some(MeterPercentPoints::new(70.0).expect("meter"));
                *meter_side = Some(UsageMeterSide::Used);
            }
        }
    }
    bridge
        .apply_publication(projection)
        .expect("fixture publication");

    let projection = bridge.desktop_projection(3).expect("desktop projection");
    assert_status_bar_export_matches_desktop(&bridge, 3);
    assert_eq!(
        projection
            .status_bar_glance_rows
            .iter()
            .map(|row| row.surface_id.as_str())
            .collect::<Vec<_>>(),
        ["claude", "codex"]
    );
    assert_eq!(
        projection.status_bar_glance_rows[0]
            .glance_meter_side
            .as_deref(),
        Some("used")
    );
    bridge.shutdown().expect("shutdown");
}

#[test]
fn desktop_projection_ranks_unrounded_remaining_overages_independent_of_input_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge =
        open_bridge_with_surfaces(dir.path(), vec!["codex".to_owned(), "claude".to_owned()]);
    bridge
        .apply_publication(fixture_with_provider_glances(&[
            ("anthropic", 0, Some(1_800_050_000), 150.0),
            ("openai", 1, Some(1_800_050_000), 100.125),
        ]))
        .expect("same-reset overage fixture");

    let projection = bridge.desktop_projection(3).expect("desktop projection");
    assert_status_bar_export_matches_desktop(&bridge, 3);
    assert_eq!(
        projection
            .status_bar_glance_rows
            .iter()
            .map(|row| row.surface_id.as_str())
            .collect::<Vec<_>>(),
        ["codex", "claude"]
    );
    assert_eq!(
        projection.status_bar_glance_rows[0].glance_remaining_percent,
        Some(100.125)
    );
    assert_eq!(
        projection.status_bar_glance_rows[1].glance_remaining_percent,
        Some(150.0)
    );
    assert_eq!(
        projection.status_bar_glance_rows[0].glance_meter_percent,
        Some(100.0)
    );
    assert_eq!(
        projection.status_bar_glance_rows[1].glance_meter_percent,
        Some(100.0)
    );
    bridge.shutdown().expect("shutdown");
}

#[test]
fn desktop_projection_ranks_status_rows_before_cap_and_preserves_full_provider_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge_with_surfaces(
        dir.path(),
        vec![
            "codex".to_owned(),
            "claude".to_owned(),
            "grok".to_owned(),
            "zai".to_owned(),
        ],
    );
    bridge
        .apply_publication(fixture_with_provider_glances(&[
            ("openai", 0, Some(1_800_070_000), 10.0),
            ("anthropic", 1, Some(1_800_050_000), 80.0),
            ("xai", 2, Some(1_800_050_000), 32.0),
            ("zai", 3, Some(1_800_050_000), 32.0),
        ]))
        .expect("fixture publication");

    let projection = bridge.desktop_projection(3).expect("desktop projection");
    assert_status_bar_export_matches_desktop(&bridge, 3);
    assert_eq!(
        assert_status_bar_export_matches_desktop(&bridge, 2).len(),
        2
    );
    assert_eq!(
        projection
            .providers
            .iter()
            .map(|provider| provider.group.surface_id.as_str())
            .collect::<Vec<_>>(),
        vec!["codex", "claude", "grok", "zai"]
    );
    assert_eq!(
        projection
            .glance_rows
            .iter()
            .map(|row| row.surface_id.as_str())
            .collect::<Vec<_>>(),
        vec!["codex", "claude", "grok", "zai"]
    );
    assert_eq!(
        projection
            .status_bar_glance_rows
            .iter()
            .map(|row| row.surface_id.as_str())
            .collect::<Vec<_>>(),
        vec!["grok", "zai", "claude"]
    );
    bridge.shutdown().expect("shutdown");
}

#[test]
fn desktop_projection_ranks_unknown_status_reset_after_known_resets() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge_with_surfaces(
        dir.path(),
        vec!["codex".to_owned(), "claude".to_owned(), "amp".to_owned()],
    );
    bridge
        .apply_publication(fixture_with_provider_glances(&[
            ("openai", 0, None, 5.0),
            ("anthropic", 1, Some(1_800_070_000), 80.0),
            ("amp", 2, Some(1_800_050_000), 70.0),
        ]))
        .expect("fixture publication");

    let projection = bridge.desktop_projection(3).expect("desktop projection");
    assert_status_bar_export_matches_desktop(&bridge, 3);
    assert_eq!(
        projection
            .status_bar_glance_rows
            .iter()
            .map(|row| row.surface_id.as_str())
            .collect::<Vec<_>>(),
        vec!["amp", "claude", "codex"]
    );
    bridge.shutdown().expect("shutdown");
}

#[test]
fn passive_bridge_reads_only_cached_broker_projection() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    bridge
        .apply_publication(fixture_projection())
        .expect("fixture publication");

    bridge
        .refresh(Some("codex".to_owned()), true)
        .expect("passive refresh");
    let snapshot = bridge.snapshot("codex".to_owned()).expect("snapshot");
    assert_eq!(snapshot.account_label, "alice@example.test");
    assert_eq!(snapshot.status, "fresh");
    assert_eq!(snapshot.buckets[0].remaining_percent, Some(64.0));
    assert_eq!(bridge.refresh_floor_secs().expect("floor"), 120);
    bridge.shutdown().expect("shutdown");
}

#[test]
fn passive_ffi_attach_does_not_dispatch_the_fake_broker_executor() {
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
                message: "unexpected probe in passive FFI fixture".to_owned(),
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
    .expect("start fake broker");
    let before = client.current_projection().expect("initial publication");

    let bridge = open_bridge(dir.path());
    bridge.refresh(None, false).expect("nonforced refresh");
    bridge
        .refresh(None, true)
        .expect("force refresh stays disabled");
    let after = client.current_projection().expect("latest publication");
    assert_eq!(before.projection_id, after.projection_id);
    assert_eq!(executor.0.load(Ordering::SeqCst), 0);
    bridge.shutdown().expect("shutdown");
}

#[test]
fn native_selection_and_details_keep_broker_canonical_identity_and_types() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    bridge
        .apply_publication(fixture_projection())
        .expect("fixture publication");

    let accounts = bridge
        .list_accounts(Some("codex".to_owned()))
        .expect("accounts");
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0].account_key, "canonical-openai-a");
    assert!(accounts[0].selected);
    let snapshot = bridge.snapshot("codex".to_owned()).expect("typed snapshot");
    assert!(
        snapshot
            .detail_presentation
            .rows
            .iter()
            .any(|row| row.row_id == "weekly-group" && row.display_label.contains("weekly"))
    );

    bridge
        .set_selected_account("codex".to_owned(), "canonical-openai-b".to_owned())
        .expect("select canonical account");
    let projection = bridge.desktop_projection(3).expect("desktop projection");
    let codex = projection
        .providers
        .iter()
        .find(|provider| provider.group.surface_id == "codex")
        .expect("codex provider");
    assert_eq!(codex.selected_account_route.status, "available");
    assert_eq!(
        codex.selected_account_route.account_key.as_deref(),
        Some("canonical-openai-b")
    );
    assert_eq!(codex.selected_usage.account_label, "bob@example.test");

    bridge
        .set_selected_account("codex".to_owned(), "unknown-account".to_owned())
        .expect_err("reject nonmember canonical id");
    bridge.shutdown().expect("shutdown");
}

#[test]
fn unavailable_selected_account_is_excluded_from_status_bar_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge_with_surfaces(dir.path(), vec!["codex".to_owned()]);
    let mut available = fixture_projection();
    let sibling_window = &mut available.providers[0].accounts[1].windows[0];
    sibling_window.value_label = "23% left".to_owned();
    sibling_window.remaining_percent = Some(RawPercentPoints::new(23.0).expect("sibling quota"));
    sibling_window.meter_percent = Some(MeterPercentPoints::new(23.0).expect("sibling meter"));
    bridge
        .apply_publication(available.clone())
        .expect("publish selected and sibling accounts");

    bridge
        .set_selected_account("codex".to_owned(), "canonical-openai-b".to_owned())
        .expect("select sibling account");
    let sibling_rows = bridge
        .status_bar_provider_glance_rows(3)
        .expect("sibling status-bar rows");
    assert_eq!(sibling_rows.len(), 1);
    assert_eq!(sibling_rows[0].account_label, "bob@example.test");
    assert_eq!(sibling_rows[0].glance_remaining_percent, Some(23.0));

    bridge
        .set_selected_account("codex".to_owned(), "canonical-openai-a".to_owned())
        .expect("select canonical account");
    let selected_rows = bridge
        .status_bar_provider_glance_rows(3)
        .expect("selected status-bar rows");
    assert_eq!(selected_rows.len(), 1);
    assert_eq!(selected_rows[0].account_label, "alice@example.test");
    assert_eq!(selected_rows[0].glance_remaining_percent, Some(64.0));

    let mut absent = available.clone();
    absent.projection_id = "fixture-canonical-account-unavailable".to_owned();
    absent.broker_generation += 1;
    absent.providers[0]
        .accounts
        .retain(|account| account.canonical_account_id != "canonical-openai-a");
    absent.providers[0].accounts[0].rank = 0;
    bridge
        .apply_publication(absent)
        .expect("publish sibling without selected account");

    let unavailable = bridge
        .desktop_projection(3)
        .expect("unavailable projection");
    let codex = unavailable
        .providers
        .iter()
        .find(|provider| provider.group.surface_id == "codex")
        .expect("Codex provider remains visible");
    assert_eq!(codex.selected_account_route.status, "unavailable");
    assert_eq!(
        codex.selected_account_route.notice.as_deref(),
        Some("The selected account is not in the current broker publication")
    );
    assert_eq!(
        codex.selected_account_route.account_key.as_deref(),
        Some("canonical-openai-a")
    );
    let accounts = bridge
        .list_accounts(Some("codex".to_owned()))
        .expect("remaining sibling account");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].account_key, "canonical-openai-b");
    assert!(!accounts[0].selected);
    assert!(unavailable.status_bar_glance_rows.is_empty());
    assert!(
        bridge
            .status_bar_provider_glance_rows(3)
            .expect("unavailable status-bar rows")
            .is_empty()
    );
    assert_eq!(
        bridge
            .compact_status_bar_label_for("codex".to_owned())
            .expect("unavailable compact label"),
        None
    );

    let mut restored = available;
    restored.projection_id = "fixture-canonical-account-restored".to_owned();
    restored.broker_generation = 3;
    bridge
        .apply_publication(restored)
        .expect("republish selected account and sibling");
    let restored = bridge.desktop_projection(3).expect("restored projection");
    let codex = restored
        .providers
        .iter()
        .find(|provider| provider.group.surface_id == "codex")
        .expect("Codex provider restored");
    assert_eq!(codex.selected_account_route.status, "available");
    assert_eq!(
        codex.selected_account_route.account_key.as_deref(),
        Some("canonical-openai-a")
    );
    let rows = bridge
        .status_bar_provider_glance_rows(3)
        .expect("restored status-bar rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].account_label, "alice@example.test");
    assert_eq!(rows[0].glance_remaining_percent, Some(64.0));
    bridge.shutdown().expect("shutdown");
}

#[test]
fn projection_glance_preserves_percent_and_reset_preferences() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    bridge
        .apply_publication(fixture_projection())
        .expect("fixture publication");
    bridge
        .set_format_prefs(UsageFormatPrefsDto {
            percent_style: "used".to_owned(),
            reset_style: "exact_clock".to_owned(),
        })
        .expect("format preferences");

    let codex = bridge
        .provider_glance_rows()
        .expect("glance rows")
        .into_iter()
        .find(|row| row.surface_id == "codex")
        .expect("codex glance row");
    assert_eq!(codex.bar_label, "36%");
    assert_eq!(codex.headline, "gpt-5 36% used");
    assert!(codex.reset_label.is_some());
    assert!(codex.exact_reset.is_some());

    assert_eq!(
        bridge
            .compact_status_bar_label_for("codex".to_owned())
            .expect("compact label")
            .as_deref(),
        Some("Cx 36%")
    );
    bridge.shutdown().expect("shutdown");
}

#[test]
fn nonforced_refresh_reads_cache_and_resets_only_the_local_floor() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    bridge
        .apply_publication(fixture_projection())
        .expect("fixture publication");
    let before = bridge
        .with_state(|state| Ok(state.runtime.projection().projection_id.clone()))
        .expect("before id");

    bridge.refresh(None, false).expect("nonforced cached read");
    let after = bridge
        .with_state(|state| Ok(state.runtime.projection().projection_id.clone()))
        .expect("after id");
    assert_eq!(before, after);
    assert!(!bridge.refresh_due().expect("refresh due"));
    bridge.shutdown().expect("shutdown");
}

#[test]
fn live_ffi_explicit_refresh_does_not_probe_in_the_client_process() {
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
    let _client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(dir.path().to_owned()),
        executor_trait,
    )
    .expect("start fake broker");
    let bridge = UsageMenuBarBridge::create();
    bridge
        .open_runtime(OpenConfig {
            data_dir_override: Some(dir.path().display().to_string()),
            config_root_override: Some(dir.path().join("config").display().to_string()),
            refresh_floor_secs: 120,
            enabled_surface_ids: vec!["codex".to_owned()],
            allow_live_probes: true,
        })
        .expect("attach live bridge to existing fake broker");
    bridge
        .refresh(Some("codex".to_owned()), false)
        .expect("periodic cache read");
    assert_eq!(executor.0.load(Ordering::SeqCst), 0);

    bridge
        .refresh(Some("codex".to_owned()), true)
        .expect("explicit operator refresh");
    assert_eq!(executor.0.load(Ordering::SeqCst), 0);
    bridge.shutdown().expect("shutdown");
}
