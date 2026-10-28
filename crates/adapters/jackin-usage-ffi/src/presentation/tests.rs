use super::*;
use crate::UsageFormatPrefsDto;
use jackin_protocol::control::{
    CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit, MeterPercentPoints,
    PercentObservationOrigin, ProviderQuotaState, RawPercentPoints, UsageMeterSide,
};
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageFreshnessV1, UsageIdentityKindV1, UsageLifecycleV1, UsageLimitWindowV1,
    UsageMembershipStateV1, UsageProjectionRefreshStateV1, UsageProjectionSchemaV2,
    UsageProjectionV1, UsageProviderV1, UsageQuotaStateV1, UsageWindowCategoryV1,
};
use jackin_protocol::usage_broker::{
    UsageCalendarPeriodV1, UsageFreshnessPhaseV1, UsageMetricGroupKindV1, UsageMetricGroupV1,
    UsageMetricPeriodV1, UsageMetricScopeV1, UsageMetricValueV1,
};
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

#[test]
fn public_metric_window_detail_preserves_full_count_observations() {
    let mut fixture =
        LocalUsageFixture::start_with_metric_groups(Vec::new(), metric_window_groups());
    assert_primary_metric_descriptor(&fixture);

    let view = fixture
        .bridge
        .snapshot("openrouter".to_owned())
        .expect("public OpenRouter snapshot");
    assert_metric_window_public_controls(&view);
    assert_metric_window_detail_labels(&view);

    fixture
        .stop_and_join()
        .expect("local usage service stops and joins");
}

#[test]
fn desktop_projection_preserves_provider_order_and_caps_status_by_reset() {
    let mut fixture = LocalUsageFixture::start_with_projection(
        provider_ordering_projection(),
        vec![
            "codex".to_owned(),
            "claude".to_owned(),
            "grok".to_owned(),
            "zai".to_owned(),
        ],
    );
    let projection = fixture
        .bridge
        .desktop_projection(3)
        .expect("public desktop projection");

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

    fixture
        .stop_and_join()
        .expect("local usage service stops and joins");
}

#[test]
fn desktop_projection_ranks_unknown_status_reset_after_known_resets() {
    let mut fixture = LocalUsageFixture::start_with_projection(
        unknown_reset_ordering_projection(),
        vec!["codex".to_owned(), "claude".to_owned(), "amp".to_owned()],
    );
    let projection = fixture
        .bridge
        .desktop_projection(3)
        .expect("public desktop projection");

    assert_eq!(
        projection
            .status_bar_glance_rows
            .iter()
            .map(|row| row.surface_id.as_str())
            .collect::<Vec<_>>(),
        vec!["amp", "claude", "codex"]
    );

    fixture
        .stop_and_join()
        .expect("local usage service stops and joins");
}

#[test]
fn projection_glance_preserves_percent_and_reset_preferences() {
    let mut fixture = LocalUsageFixture::start_with_projection(
        format_preferences_projection(),
        vec!["codex".to_owned()],
    );
    fixture
        .bridge
        .set_format_prefs(UsageFormatPrefsDto {
            percent_style: "used".to_owned(),
            reset_style: "exact_clock".to_owned(),
        })
        .expect("format preferences");

    let codex = fixture
        .bridge
        .provider_glance_rows()
        .expect("glance rows")
        .into_iter()
        .find(|row| row.surface_id == "codex")
        .expect("codex glance row");
    assert_eq!(codex.bar_label, "36%");
    assert_eq!(codex.headline, "gpt-5 36% used");
    assert!(codex.reset_label.is_some());
    assert_eq!(
        codex.reset_label,
        Some(format!(
            "Resets {}",
            jackin_usage_provider_core::local_timestamp_label(FAR_FUTURE_RESET)
        ))
    );
    assert!(codex.exact_reset.is_some());

    assert_eq!(
        fixture
            .bridge
            .compact_status_bar_label_for("codex".to_owned())
            .expect("compact label")
            .as_deref(),
        Some("Cx 36%")
    );

    fixture
        .stop_and_join()
        .expect("local usage service stops and joins");
}

#[test]
fn cached_refresh_tracks_publication_across_service_restart() {
    let initial_projection = format_preferences_projection();
    let mut fixture = LocalUsageFixture::start_with_projection(
        initial_projection.clone(),
        vec!["codex".to_owned()],
    );

    assert!(fixture.bridge.refresh_due().expect("initial refresh due"));
    fixture
        .bridge
        .set_refresh_floor_secs(120)
        .expect("set refresh floor");
    assert_eq!(
        fixture.bridge.refresh_floor_secs().expect("refresh floor"),
        120
    );
    assert!(fixture.bridge.refresh_due().expect("refresh remains due"));
    assert_cached_fixture_snapshot(&fixture);

    let initial_generation = fixture
        .bridge
        .desktop_projection(3)
        .expect("initial desktop projection")
        .generation;
    let initial_events = fixture
        .bridge
        .next_events(0, 256)
        .expect("initial public events");
    assert!(!initial_events.resync_required);
    assert!(
        initial_events
            .events
            .iter()
            .any(|event| event.kind == "runtime_opened")
    );
    let mut cursor = initial_events.next_cursor;

    fixture
        .bridge
        .refresh(None, false)
        .expect("nonforced cached refresh");
    assert!(!fixture.bridge.refresh_due().expect("refresh no longer due"));
    assert_eq!(
        fixture.bridge.refresh_floor_secs().expect("refresh floor"),
        120
    );
    assert_cached_fixture_snapshot(&fixture);
    assert_eq!(
        fixture
            .bridge
            .desktop_projection(3)
            .expect("unchanged desktop projection")
            .generation,
        initial_generation
    );
    let unchanged_events = fixture
        .bridge
        .next_events(cursor, 256)
        .expect("events after unchanged refresh");
    assert!(!unchanged_events.resync_required);
    assert!(
        !unchanged_events
            .events
            .iter()
            .any(|event| event.kind == "projection_published")
    );
    cursor = unchanged_events.next_cursor;

    fixture
        .bridge
        .refresh(Some("codex".to_owned()), true)
        .expect("passive forced refresh with live probes disabled");
    assert!(!fixture.bridge.refresh_due().expect("refresh no longer due"));
    assert_cached_fixture_snapshot(&fixture);
    assert_eq!(
        fixture
            .bridge
            .desktop_projection(3)
            .expect("passive desktop projection")
            .generation,
        initial_generation
    );
    let passive_events = fixture
        .bridge
        .next_events(cursor, 256)
        .expect("events after passive refresh");
    assert!(!passive_events.resync_required);
    assert!(
        !passive_events
            .events
            .iter()
            .any(|event| event.kind == "projection_published")
    );
    cursor = passive_events.next_cursor;

    fixture
        .stop_service_only()
        .expect("stop local service without closing the bridge");
    let mut published_projection = initial_projection;
    published_projection.projection_id = "fixture-projection-2".to_owned();
    published_projection.broker_generation = 2;
    for provider in &mut published_projection.providers {
        provider.freshness.generation = 2;
        for account in &mut provider.accounts {
            account.freshness.generation = 2;
        }
    }
    seed_projection(fixture.directory.path(), published_projection);
    fixture
        .start_service()
        .expect("restart local service on the same fixture paths");

    fixture
        .bridge
        .refresh(None, false)
        .expect("refresh changed public projection");
    assert!(!fixture.bridge.refresh_due().expect("refresh no longer due"));
    assert_eq!(
        fixture.bridge.refresh_floor_secs().expect("refresh floor"),
        120
    );
    assert_cached_fixture_snapshot(&fixture);
    assert_eq!(
        fixture
            .bridge
            .desktop_projection(3)
            .expect("updated desktop projection")
            .generation,
        2
    );
    let published_events = fixture
        .bridge
        .next_events(cursor, 256)
        .expect("events after changed projection");
    assert!(!published_events.resync_required);
    assert!(published_events.events.iter().any(|event| {
        event.kind == "projection_published"
            && event.detail.as_deref() == Some("fixture-projection-2")
    }));

    fixture
        .stop_and_join()
        .expect("local usage service stops and joins");
}

fn assert_cached_fixture_snapshot(fixture: &LocalUsageFixture) {
    let snapshot = fixture
        .bridge
        .snapshot("codex".to_owned())
        .expect("public cached snapshot");
    assert_eq!(snapshot.account_label, "alice@example.test");
    assert_eq!(snapshot.status, "fresh");
    assert_eq!(
        snapshot
            .buckets
            .first()
            .and_then(|bucket| bucket.remaining_percent),
        Some(64.0)
    );
}

fn metric_window_groups() -> Vec<UsageMetricGroupV1> {
    let mut count_group = metric_window_group(
        "metric-count-model",
        0,
        "Model requests",
        Some(request_count(Some(2), Some(10), Some(7))),
    );
    count_group.scope = UsageMetricScopeV1 {
        service: Some("chat/completions".to_owned()),
        model: Some("gpt-4o".to_owned()),
        ..UsageMetricScopeV1::default()
    };
    let unknown_count_group = metric_window_group(
        "metric-count-unknown",
        1,
        "Unknown requests",
        Some(request_count(None, None, None)),
    );
    let no_count_group = metric_window_group("metric-count-none", 2, "Requests fallback", None);
    let mut exhausted_group = metric_window_group(
        "metric-count-exhausted",
        3,
        "Exhausted requests",
        Some(request_count(Some(2), None, Some(7))),
    );
    exhausted_group.quota_state = UsageQuotaStateV1::Exhausted;
    exhausted_group.scope = UsageMetricScopeV1 {
        service: Some("chat/completions".to_owned()),
        model: Some("gpt-4o".to_owned()),
        pool: Some("priority".to_owned()),
        ..UsageMetricScopeV1::default()
    };
    exhausted_group.value = UsageMetricValueV1::Window {
        remaining_percent: Some(RawPercentPoints::new(75.0).expect("valid raw percentage")),
        remaining_percent_origin: Some(PercentObservationOrigin::Derived),
        used_percent: Some(RawPercentPoints::new(25.0).expect("valid raw percentage")),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        meter_percent: Some(MeterPercentPoints::ZERO),
        meter_side: Some(UsageMeterSide::Remaining),
        count_quota: Some(request_count(Some(2), None, Some(7))),
        provider_quota_state: Some(ProviderQuotaState::Exhausted),
        money_used_percent: None,
        period: UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Daily,
        },
        unit: Some("requests".to_owned()),
    };

    vec![
        count_group,
        unknown_count_group,
        no_count_group,
        exhausted_group,
        metric_spend_cap_group(),
    ]
}

fn metric_spend_cap_group() -> UsageMetricGroupV1 {
    UsageMetricGroupV1 {
        group_id: "metric-spend-cap-control".to_owned(),
        rank: 4,
        kind: UsageMetricGroupKindV1::SpendCap,
        label: "Precise spend".to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: Some(1_780_000_000),
        fetched_at_epoch: 1_780_000_000,
        last_success_at_epoch: Some(1_780_000_000),
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
    }
}

fn assert_primary_metric_descriptor(fixture: &LocalUsageFixture) {
    let desktop = fixture
        .bridge
        .desktop_projection(3)
        .expect("public OpenRouter desktop projection");
    let provider = desktop
        .providers
        .iter()
        .find(|provider| provider.group.surface_id == "openrouter")
        .expect("public OpenRouter provider projection");
    assert_eq!(provider.selected_account_route.status, "available");
    let account = provider
        .group
        .accounts
        .iter()
        .find(|account| account.account_key == "count_fixture")
        .expect("public fixture account descriptor");
    let count = account
        .count_quota
        .as_ref()
        .expect("public metric-window Count tuple");
    assert_eq!(
        (count.used, count.limit, count.remaining),
        (Some(2), Some(10), Some(7))
    );
    assert_eq!(count.unit, "requests");
    assert_eq!(count.period, "utc_daily");
    assert_eq!(count.provenance, "provider_reported");
    assert_eq!(account.remaining_label, "7 requests left · daily");
}

fn assert_metric_window_public_controls(view: &UsageViewDto) {
    let metric_rows = view
        .detail_presentation
        .rows
        .iter()
        .filter(|row| row.kind == "bucket")
        .collect::<Vec<_>>();
    assert_eq!(metric_rows.len(), 5);
    let mut row_ids = metric_rows
        .iter()
        .map(|row| row.row_id.as_str())
        .collect::<Vec<_>>();
    row_ids.sort_unstable();
    assert_eq!(
        row_ids,
        vec![
            "metric-count-exhausted",
            "metric-count-model",
            "metric-count-none",
            "metric-count-unknown",
            "metric-spend-cap-control",
        ]
    );
    let count_row = metric_detail_row(view, "metric-count-model");
    assert_eq!(count_row.kind, "bucket");
    assert_eq!(count_row.label, "Model requests");
    assert_eq!(
        count_row.layout_lines[0].trailing.as_deref(),
        Some("service: chat/completions · model: gpt-4o")
    );
    let unknown_count_row = metric_detail_row(view, "metric-count-unknown");
    assert_eq!(unknown_count_row.kind, "bucket");
    assert_eq!(unknown_count_row.label, "Unknown requests");
    let no_count_row = metric_detail_row(view, "metric-count-none");
    assert_eq!(no_count_row.kind, "bucket");
    assert_eq!(no_count_row.label, "Requests fallback");
    let exhausted_row = metric_detail_row(view, "metric-count-exhausted");
    assert_eq!(exhausted_row.kind, "bucket");
    assert_eq!(exhausted_row.label, "Exhausted requests");
    assert_eq!(
        exhausted_row.layout_lines[0].trailing.as_deref(),
        Some("service: chat/completions · model: gpt-4o · pool: priority")
    );
    assert!(
        exhausted_row
            .display_label
            .starts_with("Exhausted · Reported used: 25% · Derived remaining: 75%")
    );
    assert_eq!(exhausted_row.severity, "danger");
    assert_eq!(exhausted_row.meter_percent, Some(0.0));
    assert_eq!(exhausted_row.meter_side.as_deref(), Some("remaining"));
    let spend_cap_row = metric_detail_row(view, "metric-spend-cap-control");
    assert_eq!(
        spend_cap_row.display_label,
        "spent $0.100000000000000000 · cap $0.100000000000000001 · remaining $0.000000000000000001"
    );
}

fn assert_metric_window_detail_labels(view: &UsageViewDto) {
    let count_row = metric_detail_row(view, "metric-count-model");
    let unknown_count_row = metric_detail_row(view, "metric-count-unknown");
    let no_count_row = metric_detail_row(view, "metric-count-none");
    let exhausted_row = metric_detail_row(view, "metric-count-exhausted");
    assert_eq!(
        unknown_count_row.display_label,
        "daily · quota available · Reported count (requests/utc_daily): used unknown, limit unknown, remaining unknown"
    );
    assert_eq!(no_count_row.display_label, "daily · quota available");
    assert_eq!(
        count_row.display_label,
        "Reported count (requests/utc_daily): used 2, limit 10, remaining 7 · daily"
    );
    assert_eq!(
        exhausted_row.display_label,
        "Exhausted · Reported used: 25% · Derived remaining: 75% · Reported count (requests/utc_daily): used 2, limit unknown, remaining 7 · daily"
    );
}

fn metric_detail_row<'a>(view: &'a UsageViewDto, row_id: &str) -> &'a UsageDetailRowDto {
    view.detail_presentation
        .rows
        .iter()
        .find(|row| row.row_id == row_id)
        .expect("public metric detail row")
}

fn metric_window_group(
    group_id: &str,
    rank: u32,
    label: &str,
    count_quota: Option<CountQuota>,
) -> UsageMetricGroupV1 {
    UsageMetricGroupV1 {
        group_id: group_id.to_owned(),
        rank,
        kind: UsageMetricGroupKindV1::Window,
        label: label.to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: Some(1_780_000_000),
        fetched_at_epoch: 1_780_000_000,
        last_success_at_epoch: Some(1_780_000_000),
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::Window {
            remaining_percent: None,
            remaining_percent_origin: None,
            used_percent: None,
            used_percent_origin: None,
            meter_percent: None,
            meter_side: None,
            count_quota,
            provider_quota_state: None,
            money_used_percent: None,
            period: UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Daily,
            },
            unit: Some("requests".to_owned()),
        },
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    }
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

    let money_collision = view
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Count with source and money")
        .expect("count and money collision bucket");
    assert_eq!(money_collision.used_percent, Some(37.5));
    assert_eq!(
        money_collision.used_percent_origin.as_deref(),
        Some("provider_reported")
    );
    assert_eq!(money_collision.remaining_percent, Some(62.5));
    assert_eq!(
        money_collision.remaining_percent_origin.as_deref(),
        Some("derived")
    );
    assert_eq!(money_collision.meter_percent, Some(12.5));
    assert_eq!(money_collision.meter_side.as_deref(), Some("used"));
    assert_eq!(
        compact_label(view, "Count with source and money"),
        "Reported used: 37.5% · Derived remaining: 62.5% · Money ratio: 12.5% used"
    );

    let opaque_count = view
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Opaque count")
        .expect("opaque count bucket");
    assert_eq!(opaque_count.used_percent, None);
    assert_eq!(opaque_count.remaining_percent, None);
    assert_eq!(opaque_count.meter_percent, None);
    assert_eq!(opaque_count.meter_side, None);
    assert_eq!(compact_label(view, "Opaque count"), "6 requests left");

    let money_detail_row = view
        .detail_presentation
        .rows
        .iter()
        .find(|row| row.row_id == "count-money-collision")
        .expect("count and money collision detail row");
    assert_eq!(money_detail_row.meter_percent, Some(12.5));
    assert_eq!(money_detail_row.meter_side.as_deref(), Some("used"));
    let opaque_detail_row = view
        .detail_presentation
        .rows
        .iter()
        .find(|row| row.row_id == "count-opaque-label")
        .expect("opaque count detail row");
    assert_eq!(opaque_detail_row.meter_percent, None);
    assert_eq!(opaque_detail_row.meter_side, None);
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
    let opaque_detail = detail_label(view, "count-opaque-label");
    assert_eq!(
        opaque_detail,
        "Provider status note [opaque-42] · Reported count (requests/utc_daily): used 4, limit 10, remaining 6"
    );
    assert_eq!(opaque_detail.matches("Reported count").count(), 1);
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
    assert_eq!(
        detail_label(view, "count-money-collision"),
        "Reported used: 37.5% · Derived remaining: 62.5% · Money ratio: 12.5% used · Reported count (requests/utc_daily): used 2, limit 10, remaining 7"
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
        (
            "count-money-collision",
            "Count with source and money",
            "Provider supplied raw and Money values",
            Some(request_count(Some(2), Some(10), Some(7))),
        ),
        (
            "count-opaque-label",
            "Opaque count",
            "Provider status note [opaque-42]",
            Some(request_count(Some(4), Some(10), Some(6))),
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

    let money_collision = windows
        .iter_mut()
        .find(|window| window.window_id == "count-money-collision")
        .expect("count and money collision fixture window");
    money_collision.used_percent = Some(RawPercentPoints::new(37.5).expect("valid raw percentage"));
    money_collision.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    money_collision.remaining_percent =
        Some(RawPercentPoints::new(62.5).expect("valid raw percentage"));
    money_collision.remaining_percent_origin = Some(PercentObservationOrigin::Derived);
    money_collision.money_used_percent =
        Some(RawPercentPoints::new(12.5).expect("valid Money ratio"));
    money_collision.meter_percent =
        Some(MeterPercentPoints::new(12.5).expect("valid meter percentage"));
    money_collision.meter_side = Some(UsageMeterSide::Used);

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

    fn retain_path(&mut self) -> Option<PathBuf> {
        self.0.take()
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
        Self::start_with_metric_groups(windows, Vec::new())
    }

    fn start_with_metric_groups(
        windows: Vec<UsageLimitWindowV1>,
        metric_groups: Vec<UsageMetricGroupV1>,
    ) -> Self {
        Self::start_with_projection(
            fixture_projection(windows, metric_groups),
            vec!["openrouter".to_owned()],
        )
    }

    fn start_with_projection(
        projection: UsageProjectionV1,
        enabled_surface_ids: Vec<String>,
    ) -> Self {
        let directory = FixtureDirectory::create();
        seed_projection(directory.path(), projection);
        let resolved_data_dir = fs::canonicalize(directory.path()).expect("canonical fixture root");
        let socket_path = resolved_data_dir.join("usage-broker/run/usage-broker.sock");
        assert!(socket_path.as_os_str().as_encoded_bytes().len() < 104);

        let config = UsageBrokerConfig::for_data_dir(directory.path().to_path_buf());
        let client = config.client();
        let mut fixture = Self {
            directory,
            client,
            bridge: crate::UsageMenuBarBridge::create(),
            worker: None,
        };
        fixture
            .start_service()
            .expect("start joined local-only usage service");
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
                enabled_surface_ids,
                allow_live_probes: false,
            })
            .expect("open public bridge without live probes");
        fixture
    }

    fn start_service(&mut self) -> Result<(), String> {
        if self.worker.is_some() {
            return Err("local usage service worker is already running".to_owned());
        }
        let mut config = UsageBrokerConfig::for_data_dir(self.directory.path().to_path_buf());
        config.idle_exit = Duration::from_secs(30);
        self.worker = Some(
            jackin_telemetry::spawn::thread_joined_named(
                "count-detail-fixture".to_owned(),
                move || run_usage_monitor_service(config),
            )
            .map_err(|error| format!("spawn joined local-only usage service: {error:?}"))?,
        );
        self.wait_for_service()
    }

    fn wait_for_service(&self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if matches!(
                self.client.monitor(MonitorOperation::ServiceStatus),
                Ok(MonitorReply::ServiceStatus { .. })
            ) {
                return Ok(());
            }
            if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
                return Err("local usage service exited before becoming ready".to_owned());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("local usage service did not become ready before timeout".to_owned());
            }
            std::thread::park_timeout(remaining.min(Duration::from_millis(10)));
        }
    }

    fn stop_service_only(&mut self) -> Result<(), String> {
        self.stop_service_and_drain()
    }

    fn stop_service_and_drain(&mut self) -> Result<(), String> {
        let stopped = self
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
        let joined = self.join_worker_bounded();
        match (stopped, joined) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(stop_error), Ok(())) => Err(stop_error),
            (Ok(()), Err(join_error)) => Err(join_error),
            (Err(stop_error), Err(join_error)) => Err(format!("{stop_error}; {join_error}")),
        }
    }

    fn join_worker_bounded(&mut self) -> Result<(), String> {
        if self.worker.is_none() {
            return Err("local usage service worker was already joined".to_owned());
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                drop(self.worker.take());
                let retained_path = self
                    .directory
                    .retain_path()
                    .map_or_else(|| "unknown".to_owned(), |path| path.display().to_string());
                return Err(format!(
                    "local usage service did not stop within timeout; detached worker and retained fixture directory at {retained_path}"
                ));
            }
            std::thread::park_timeout(remaining.min(Duration::from_millis(10)));
        }
        let Some(worker) = self.worker.take() else {
            return Err("local usage service worker was already joined".to_owned());
        };
        match worker.join() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(format!("local usage service failed: {}", error.message)),
            Err(_) => Err("local usage service worker panicked".to_owned()),
        }
    }

    fn stop_and_join(&mut self) -> Result<(), String> {
        let bridge_shutdown = self
            .bridge
            .shutdown()
            .map_err(|error| format!("close public bridge: {error:?}"));
        let service_stop_and_join = self.stop_service_and_drain();
        let removed = self
            .directory
            .remove()
            .map_err(|error| format!("remove local usage fixture directory: {error}"));
        bridge_shutdown?;
        service_stop_and_join?;
        removed
    }
}

impl Drop for LocalUsageFixture {
    fn drop(&mut self) {
        drop(self.bridge.shutdown());
        if self.worker.is_some()
            && let Err(error) = self.stop_service_and_drain()
        {
            eprintln!("local usage fixture cleanup failed: {error}");
        }
    }
}

fn provider_ordering_projection() -> UsageProjectionV1 {
    fixture_provider_projection([
        ("openai", "OpenAI", 0_u32, Some(1_800_070_000), 10_u8),
        ("anthropic", "Anthropic", 1, Some(1_800_050_000), 80),
        ("xai", "xAI", 2, Some(1_800_050_000), 32),
        ("zai", "Z.AI", 3, Some(1_800_050_000), 32),
    ])
}

fn unknown_reset_ordering_projection() -> UsageProjectionV1 {
    fixture_provider_projection([
        ("openai", "OpenAI", 0_u32, None, 5_u8),
        ("anthropic", "Anthropic", 1, Some(1_800_070_000), 80),
        ("amp", "Amp", 2, Some(1_800_050_000), 70),
    ])
}

const FAR_FUTURE_RESET: i64 = 253_370_764_800;

fn format_preferences_projection() -> UsageProjectionV1 {
    let mut projection = fixture_projection(Vec::new(), Vec::new());
    projection.projection_id = "fixture-projection-1".to_owned();
    projection.generated_at_epoch = 1_800_000_001;
    projection.discovery_revision = "fixture-catalog-1".to_owned();
    projection.broker_instance_id = "fixture-broker".to_owned();

    let fresh = UsageFreshnessV1 {
        generation: 1,
        phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(1_800_000_000),
        retry_at_epoch: None,
        is_stale: false,
    };
    let provider = &mut projection.providers[0];
    provider.provider_id = "openai".to_owned();
    provider.display_name = "OpenAI".to_owned();
    provider.freshness = fresh.clone();

    let account = &mut provider.accounts[0];
    account.canonical_account_id = "canonical-openai-a".to_owned();
    account.display_label = "alice@example.test".to_owned();
    account.plan_label = Some("Pro".to_owned());
    account.freshness = fresh.clone();

    let mut weekly_group = metric_window_group("weekly-group", 0, "Weekly quota", None);
    weekly_group.scope = UsageMetricScopeV1 {
        model: Some("gpt-5".to_owned()),
        ..UsageMetricScopeV1::default()
    };
    weekly_group.observed_at_epoch = Some(1_800_000_000);
    weekly_group.fetched_at_epoch = 1_800_000_001;
    weekly_group.last_success_at_epoch = Some(1_800_000_000);
    weekly_group.reset_at_epoch = Some(FAR_FUTURE_RESET);
    weekly_group.value = UsageMetricValueV1::Window {
        remaining_percent: Some(RawPercentPoints::new(64.0).expect("valid remaining percentage")),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        used_percent: None,
        used_percent_origin: None,
        meter_percent: None,
        meter_side: None,
        count_quota: None,
        provider_quota_state: None,
        money_used_percent: None,
        period: UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Weekly,
        },
        unit: Some("tokens".to_owned()),
    };

    let mut weekly_window =
        fixture_window(0, "canonical-openai-a-weekly", "Weekly", "64% left", None);
    weekly_window.category = UsageWindowCategoryV1::LongRange;
    weekly_window.reset_at_epoch = Some(FAR_FUTURE_RESET);
    weekly_window.remaining_percent =
        Some(RawPercentPoints::new(64.0).expect("valid remaining percentage"));
    weekly_window.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    account.windows = vec![weekly_window.clone()];
    account.metric_groups = vec![weekly_group];

    let mut secondary_account = account.clone();
    secondary_account.canonical_account_id = "canonical-openai-b".to_owned();
    secondary_account.rank = 1;
    secondary_account.display_label = "bob@example.test".to_owned();
    secondary_account.metric_groups.clear();
    secondary_account.windows[0].window_id = "canonical-openai-b-weekly".to_owned();
    provider.accounts.push(secondary_account);

    projection
}

fn fixture_provider_projection<const N: usize>(
    provider_specs: [(&str, &str, u32, Option<i64>, u8); N],
) -> UsageProjectionV1 {
    let mut projection = fixture_projection(Vec::new(), Vec::new());
    let source = projection.providers[0].clone();
    projection.providers = provider_specs
        .into_iter()
        .map(
            |(provider_id, display_name, rank, reset_at_epoch, remaining)| {
                let mut provider = source.clone();
                provider.provider_id = provider_id.to_owned();
                provider.display_name = display_name.to_owned();
                provider.rank = rank;
                let account = provider
                    .accounts
                    .first_mut()
                    .expect("ordering fixture account");
                account.canonical_account_id = format!("{provider_id}-account");
                account.display_label = format!("{provider_id}-fixture");
                let mut window = fixture_window(
                    0,
                    &format!("{provider_id}-window"),
                    "Usage",
                    &format!("{remaining}% left"),
                    None,
                );
                window.reset_at_epoch = reset_at_epoch;
                window.remaining_percent = Some(
                    RawPercentPoints::new(f64::from(remaining))
                        .expect("valid remaining percentage"),
                );
                window.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
                window.meter_percent = Some(
                    MeterPercentPoints::new(f64::from(remaining)).expect("valid meter percentage"),
                );
                window.meter_side = Some(UsageMeterSide::Remaining);
                account.windows = vec![window];
                provider
            },
        )
        .collect();
    projection
}

fn fixture_projection(
    windows: Vec<UsageLimitWindowV1>,
    metric_groups: Vec<UsageMetricGroupV1>,
) -> UsageProjectionV1 {
    let freshness = UsageFreshnessV1 {
        generation: 1,
        phase: UsageFreshnessPhaseV1::Stale,
        last_good_at_epoch: Some(1_780_000_000),
        retry_at_epoch: None,
        is_stale: true,
    };
    let account_id = "count_fixture".to_owned();
    UsageProjectionV1 {
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
                metric_groups,
                credential_expires_at_epoch: None,
                issues: Vec::new(),
            }],
            issues: Vec::new(),
        }],
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

fn seed_projection(data_dir: &Path, projection: UsageProjectionV1) {
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
