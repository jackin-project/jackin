// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use jackin_console::tui::screens::usage::UsageScreenState;
use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, Money, QuotaBucketView, StatusSlot, UsageConfidence,
    UsageSeverity, UsageSnapshotStatus, UsageSource,
};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageAccountV1, UsageCatalogEntry, UsageCoordinationErrorKind,
    UsageFreshnessPhaseV1, UsageFreshnessV1, UsageIdentityKindV1, UsageLifecycleV1,
    UsageMembershipStateV1, UsageMetricGroupKindV1, UsageMetricValueV1,
    UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageProjectionV1, UsageProviderV1,
    UsageQuotaStateV1, UsageRefreshPhase,
};

use super::*;
use crate::coordinator::{
    FileProjectionStateStore, ProjectionStateEnvelope, ProviderProbeOutcome, UsageProviderExecutor,
};
use crate::host::{
    HostUsageProjectionConfig, HostUsageProjectionRuntime, HostUsageProjectionSelectedAccount,
    UsageBrokerConfig, UsageDestination, ensure_usage_broker_with_executor, normalize_destination,
};
use crate::usage::{provider_tabs, usage_bucket_presentation, usage_detail_presentation};

const FIXTURE_NOW: i64 = 1_800_000_000;

fn bucket(label: &str) -> QuotaBucketView {
    QuotaBucketView {
        label: label.into(),
        used_label: None,
        limit_label: None,
        remaining_percent: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}

fn view_with_buckets(
    status: UsageSnapshotStatus,
    buckets: Vec<QuotaBucketView>,
) -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: None,
        focused_provider: None,
        account: FocusedAccountHeader {
            provider_label: "Codex".into(),
            account_label: "work@example.test".into(),
            username: None,
            plan_label: None,
            credential_origin: None,
        },
        buckets,
        status,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: FIXTURE_NOW,
        updated_label: "now".into(),
        status_bar_label: "ok".into(),
        tabs: Vec::new(),
        last_error: None,
    }
}

fn window_group<'a>(groups: &'a [UsageMetricGroupV1], label: &str) -> &'a UsageMetricGroupV1 {
    groups
        .iter()
        .find(|group| group.kind == UsageMetricGroupKindV1::Window && group.label == label)
        .expect("window metric group")
}

fn window_remaining_percent(group: &UsageMetricGroupV1) -> Option<u8> {
    match &group.value {
        UsageMetricValueV1::Window {
            remaining_percent, ..
        } => remaining_percent.map(|percent| percent.get()),
        _ => None,
    }
}

fn window_used_percent(group: &UsageMetricGroupV1) -> Option<u8> {
    match &group.value {
        UsageMetricValueV1::Window { used_percent, .. } => {
            used_percent.map(|percent| percent.get())
        }
        _ => None,
    }
}

#[test]
fn window_projection_preserves_raw_overage_from_money_ratio() {
    let mut spend = bucket("Extra usage");
    spend.status_slot = Some(StatusSlot::Spend);
    spend.used_money = Some(Money::new(12_000, "USD", 2));
    spend.limit_money = Some(Money::new(10_000, "USD", 2));
    let view = view_with_buckets(UsageSnapshotStatus::Fresh, vec![spend]);
    let groups = metric_groups_for_view("canon-1", &view, None).unwrap();
    let window = window_group(&groups, "Extra usage");

    assert_eq!(window_used_percent(window), Some(100));
    assert_eq!(window.quota_state, UsageQuotaStateV1::Exhausted);
    match &window.value {
        UsageMetricValueV1::Window {
            remaining_percent,
            used_percent,
            used_raw_percent,
            ..
        } => {
            assert_eq!(remaining_percent, &None);
            assert_eq!(used_percent.map(UsagePercent::get), Some(100));
            assert_eq!(*used_raw_percent, Some(120));
        }
        other => panic!("expected window value, got {other:?}"),
    }
    window.validate(0).unwrap();
}

#[test]
fn window_projection_keeps_checked_math_without_wrap_or_fabrication() {
    let mut huge = bucket("Huge");
    huge.used_money = Some(Money::new(i64::MAX, "USD", 2));
    huge.limit_money = Some(Money::new(1, "USD", 2));
    let view = view_with_buckets(UsageSnapshotStatus::Fresh, vec![huge]);
    let groups = metric_groups_for_view("canon-1", &view, None).unwrap();
    let window = window_group(&groups, "Huge");
    match &window.value {
        UsageMetricValueV1::Window {
            used_percent,
            used_raw_percent,
            ..
        } => {
            assert_eq!(used_percent.map(UsagePercent::get), Some(100));
            assert_eq!(*used_raw_percent, Some(i32::MAX));
        }
        other => panic!("expected window value, got {other:?}"),
    }
    window.validate(0).unwrap();

    let mut mismatched = bucket("Mismatched");
    mismatched.used_money = Some(Money::new(50_00, "USD", 2));
    mismatched.limit_money = Some(Money::new(10_000, "SGD", 2));
    let view = view_with_buckets(UsageSnapshotStatus::Fresh, vec![mismatched.clone()]);
    let error = metric_groups_for_view("canon-1", &view, None)
        .expect_err("mixed currency spend-cap groups must be rejected");
    assert!(error.contains("mixes money denominations"), "{error}");
    let window = project_window_group(
        "canon-1",
        &mismatched,
        view.status,
        view.fetched_at_epoch,
        0,
    )
    .unwrap();
    match &window.value {
        UsageMetricValueV1::Window {
            used_percent,
            used_raw_percent,
            ..
        } => {
            assert_eq!(*used_percent, None);
            assert_eq!(*used_raw_percent, None);
        }
        other => panic!("expected window value, got {other:?}"),
    }
    window.validate(0).unwrap();

    let mut zero_cap = bucket("Zero cap");
    zero_cap.used_money = Some(Money::new(1, "USD", 2));
    zero_cap.limit_money = Some(Money::new(0, "USD", 2));
    let view = view_with_buckets(UsageSnapshotStatus::Fresh, vec![zero_cap]);
    let groups = metric_groups_for_view("canon-1", &view, None).unwrap();
    let window = window_group(&groups, "Zero cap");
    match &window.value {
        UsageMetricValueV1::Window {
            used_percent,
            used_raw_percent,
            ..
        } => {
            assert_eq!(*used_percent, None);
            assert_eq!(*used_raw_percent, None);
        }
        other => panic!("expected window value, got {other:?}"),
    }
    window.validate(0).unwrap();
}

#[test]
fn quota_state_keeps_permission_unknown_and_exhausted_distinct() {
    let mut login = bucket("Login");
    login.status = UsageSnapshotStatus::NeedsLogin;
    assert_eq!(quota_state(&login), UsageQuotaStateV1::NoPermission);

    let mut secret = bucket("Secret");
    secret.status = UsageSnapshotStatus::NeedsSecret;
    assert_eq!(quota_state(&secret), UsageQuotaStateV1::NoPermission);

    let mut unsupported = bucket("Unsupported");
    unsupported.status = UsageSnapshotStatus::Unsupported;
    assert_eq!(quota_state(&unsupported), UsageQuotaStateV1::Unsupported);

    let mut unavailable = bucket("Unavailable");
    unavailable.status = UsageSnapshotStatus::Unavailable;
    assert_eq!(quota_state(&unavailable), UsageQuotaStateV1::Unavailable);

    let mut error = bucket("Error");
    error.status = UsageSnapshotStatus::Error;
    assert_eq!(quota_state(&error), UsageQuotaStateV1::Error);

    let empty = bucket("Empty");
    assert_eq!(quota_state(&empty), UsageQuotaStateV1::Unknown);

    let mut exhausted = bucket("Exhausted");
    exhausted.remaining_percent = Some(0);
    assert_eq!(quota_state(&exhausted), UsageQuotaStateV1::Exhausted);

    let mut available = bucket("Available");
    available.remaining_percent = Some(57);
    assert_eq!(quota_state(&available), UsageQuotaStateV1::Available);

    let mut warn = bucket("Warn");
    warn.remaining_percent = Some(10);
    warn.severity = UsageSeverity::Warn;
    assert_eq!(quota_state(&warn), UsageQuotaStateV1::Warning);

    let mut danger = bucket("Danger");
    danger.remaining_percent = Some(10);
    danger.severity = UsageSeverity::Danger;
    assert_eq!(quota_state(&danger), UsageQuotaStateV1::Exhausted);
}

#[test]
fn broker_failures_keep_typed_lifecycle_mapping() {
    assert_eq!(
        failure_lifecycle(UsageCoordinationErrorKind::NeedsSecret),
        UsageLifecycleV1::NeedsSecret
    );
    assert_eq!(
        failure_lifecycle(UsageCoordinationErrorKind::Unauthorized),
        UsageLifecycleV1::NeedsLogin
    );
    assert_eq!(
        failure_lifecycle(UsageCoordinationErrorKind::ProtocolMismatch),
        UsageLifecycleV1::Unsupported
    );
    assert_eq!(
        failure_lifecycle(UsageCoordinationErrorKind::ProviderUnavailable),
        UsageLifecycleV1::Unavailable
    );
    assert_eq!(
        lifecycle(UsageSnapshotStatus::Fresh, UsageConfidence::Authoritative),
        UsageLifecycleV1::Available
    );
    assert_eq!(
        lifecycle(UsageSnapshotStatus::Fresh, UsageConfidence::PresenceOnly),
        UsageLifecycleV1::AgentUninitialized
    );
}

#[test]
fn metric_groups_project_window_spend_and_plan() {
    let mut weekly = bucket("Weekly");
    weekly.remaining_percent = Some(57);
    weekly.resets_at = Some(FIXTURE_NOW + 100);
    weekly.reset_label = Some("Resets soon".into());
    weekly.status_slot = Some(StatusSlot::Weekly);
    let mut spend = bucket("Extra usage");
    spend.status_slot = Some(StatusSlot::Spend);
    spend.used_money = Some(Money::new(27_00, "USD", 2));
    spend.limit_money = Some(Money::new(30_000, "USD", 2));
    spend.used_label = Some("$27.00".into());
    spend.resets_at = Some(FIXTURE_NOW + 200);
    let view = view_with_buckets(UsageSnapshotStatus::Fresh, vec![weekly, spend]);

    let groups = metric_groups_for_view("canon-1", &view, Some("Pro")).unwrap();
    assert_eq!(
        groups.iter().map(|group| group.kind).collect::<Vec<_>>(),
        [
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::SpendCap,
            UsageMetricGroupKindV1::Plan,
        ]
    );
    for (rank, group) in groups.iter().enumerate() {
        group.validate(rank).unwrap();
        assert_eq!(group.fetched_at_epoch, FIXTURE_NOW);
        assert_eq!(group.observed_at_epoch, Some(FIXTURE_NOW));
        assert_eq!(group.last_success_at_epoch, Some(FIXTURE_NOW));
        assert!(!group.is_stale);
    }
    assert_eq!(window_remaining_percent(&groups[0]), Some(57));
    assert_eq!(groups[0].reset_at_epoch, Some(FIXTURE_NOW + 100));
    assert_eq!(groups[0].renews_at_epoch, None);

    match &groups[2].value {
        UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            assert_eq!(cap, &Some(Money::new(30_000, "USD", 2)));
            assert_eq!(spent, &Some(Money::new(27_00, "USD", 2)));
            assert_eq!(remaining, &Some(Money::new(30_000 - 27_00, "USD", 2)));
        }
        other => panic!("expected spend-cap value, got {other:?}"),
    }
    assert_eq!(groups[2].reset_at_epoch, Some(FIXTURE_NOW + 200));
    assert_eq!(groups[3].quota_state, UsageQuotaStateV1::NotApplicable);
    assert_eq!(groups[3].reset_at_epoch, None);
    assert_eq!(groups[3].renews_at_epoch, None);

    let rerun = metric_groups_for_view("canon-1", &view, Some("Pro")).unwrap();
    assert_eq!(
        groups
            .iter()
            .map(|group| &group.group_id)
            .collect::<Vec<_>>(),
        rerun
            .iter()
            .map(|group| &group.group_id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn spend_group_states_cover_ratio_edges() {
    let mut over = bucket("Over");
    over.used_money = Some(Money::new(12_000, "USD", 2));
    over.limit_money = Some(Money::new(10_000, "USD", 2));
    assert_eq!(spend_quota_state(&over), UsageQuotaStateV1::Exhausted);

    let mut warn = bucket("Warn");
    warn.used_money = Some(Money::new(80_00, "USD", 2));
    warn.limit_money = Some(Money::new(10_000, "USD", 2));
    assert_eq!(spend_quota_state(&warn), UsageQuotaStateV1::Warning);

    let mut ok = bucket("Ok");
    ok.used_money = Some(Money::new(10_00, "USD", 2));
    ok.limit_money = Some(Money::new(10_000, "USD", 2));
    assert_eq!(spend_quota_state(&ok), UsageQuotaStateV1::Available);

    let mut uncapped = bucket("Uncapped");
    uncapped.used_money = Some(Money::new(10_00, "USD", 2));
    assert_eq!(
        spend_quota_state(&uncapped),
        UsageQuotaStateV1::NotApplicable
    );

    let mut cap_only = bucket("Cap only");
    cap_only.limit_money = Some(Money::new(10_000, "USD", 2));
    assert_eq!(spend_quota_state(&cap_only), UsageQuotaStateV1::Unknown);

    let mut mismatched = bucket("Mismatched");
    mismatched.used_money = Some(Money::new(10_00, "USD", 2));
    mismatched.limit_money = Some(Money::new(10_000, "SGD", 2));
    assert_eq!(spend_quota_state(&mismatched), UsageQuotaStateV1::Unknown);

    let mut login = bucket("Login");
    login.status = UsageSnapshotStatus::NeedsLogin;
    login.used_money = Some(Money::new(10_00, "USD", 2));
    login.limit_money = Some(Money::new(10_000, "USD", 2));
    assert_eq!(spend_quota_state(&login), UsageQuotaStateV1::NoPermission);
}

fn status_bucket(label: &str, status: UsageSnapshotStatus) -> QuotaBucketView {
    let mut bucket = bucket(label);
    bucket.remaining_percent = Some(57);
    bucket.status = status;
    bucket
}

fn first_metric_group(view: &FocusedUsageView) -> UsageMetricGroupV1 {
    metric_groups_for_view("canon-1", view, None)
        .unwrap()
        .into_iter()
        .next()
        .expect("one metric group")
}

#[test]
fn group_timestamps_track_view_usability() {
    let fresh = view_with_buckets(
        UsageSnapshotStatus::Fresh,
        vec![status_bucket("Weekly", UsageSnapshotStatus::Fresh)],
    );
    let group = first_metric_group(&fresh);
    assert_eq!(group.phase, UsageFreshnessPhaseV1::Current);
    assert_eq!(group.last_success_at_epoch, Some(FIXTURE_NOW));

    let stale = view_with_buckets(
        UsageSnapshotStatus::Stale,
        vec![status_bucket("Weekly", UsageSnapshotStatus::Stale)],
    );
    let group = first_metric_group(&stale);
    assert_eq!(group.phase, UsageFreshnessPhaseV1::Stale);
    assert!(group.is_stale);
    assert_eq!(group.last_success_at_epoch, Some(FIXTURE_NOW));

    let error = view_with_buckets(
        UsageSnapshotStatus::Error,
        vec![status_bucket("Weekly", UsageSnapshotStatus::Error)],
    );
    let group = first_metric_group(&error);
    assert_eq!(group.phase, UsageFreshnessPhaseV1::Failed);
    assert_eq!(group.last_success_at_epoch, None);
    assert_eq!(group.quota_state, UsageQuotaStateV1::Error);
}

fn projection_fixture(accounts: &[(&str, &str)]) -> UsageProjectionV1 {
    let freshness = UsageFreshnessV1 {
        generation: 1,
        phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(FIXTURE_NOW),
        retry_at_epoch: None,
        is_stale: false,
    };
    let accounts = accounts
        .iter()
        .enumerate()
        .map(
            |(rank, (canonical_account_id, display_label))| UsageAccountV1 {
                canonical_account_id: (*canonical_account_id).to_owned(),
                identity_kind: UsageIdentityKindV1::ProviderStableHandle,
                rank: u32::try_from(rank).unwrap(),
                display_label: (*display_label).to_owned(),
                plan_label: None,
                status_label: Some("fresh".to_owned()),
                lifecycle: UsageLifecycleV1::Available,
                freshness: freshness.clone(),
                provenance_count: 1,
                windows: Vec::new(),
                metric_groups: Vec::new(),
                credential_expires_at_epoch: None,
                issues: Vec::new(),
            },
        )
        .collect();
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "fixture:1".to_owned(),
        generated_at_epoch: FIXTURE_NOW,
        discovery_revision: "fixture-catalog".to_owned(),
        broker_instance_id: "fixture-broker".to_owned(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: vec![UsageProviderV1 {
            provider_id: "openai".to_owned(),
            display_name: "OpenAI".to_owned(),
            rank: 0,
            membership_state: UsageMembershipStateV1::Current,
            freshness,
            accounts,
            issues: Vec::new(),
        }],
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

#[test]
fn normalize_destination_keeps_exact_account_and_falls_back_after_removal() {
    let projection = projection_fixture(&[
        ("canon-account-a", "alpha@example.test"),
        ("canon-account-b", "beta@example.test"),
    ]);
    let selected = UsageDestination::Account {
        provider_id: "openai".to_owned(),
        canonical_account_id: "canon-account-b".to_owned(),
    };
    assert_eq!(
        normalize_destination(&projection, &selected),
        NormalizedUsageDestination {
            destination: selected.clone(),
            notice: None,
        }
    );

    let removed = UsageDestination::Account {
        provider_id: "openai".to_owned(),
        canonical_account_id: "removed".to_owned(),
    };
    assert_eq!(
        normalize_destination(&projection, &removed),
        NormalizedUsageDestination {
            destination: UsageDestination::Overview,
            notice: Some("Selected account is no longer available.".to_owned()),
        }
    );
}

#[test]
fn projection_runtime_preserves_exact_selection_when_account_disappears() {
    let temp = tempfile::tempdir().unwrap();
    let mut projection = projection_fixture(&[
        ("canon-account-a", "alpha@example.test"),
        ("canon-account-b", "beta@example.test"),
    ]);
    let mut runtime = HostUsageProjectionRuntime::open(
        projection.clone(),
        HostUsageProjectionConfig::under_data_dir(temp.path()),
    )
    .unwrap();

    let initial = runtime.account_inventory(Some("codex")).unwrap();
    assert_eq!(initial.len(), 2);
    assert_eq!(
        initial
            .iter()
            .filter(|account| account.selected)
            .map(|account| account.account.canonical_account_id.as_str())
            .collect::<Vec<_>>(),
        ["canon-account-a"]
    );
    runtime
        .set_selected_account("codex", Some("canon-account-b"))
        .unwrap();
    assert!(matches!(
        runtime
            .provider_presentation("codex")
            .unwrap()
            .selected_account,
        HostUsageProjectionSelectedAccount::Available {
            canonical_account_id: "canon-account-b",
            ..
        }
    ));

    projection.providers[0]
        .accounts
        .retain(|account| account.canonical_account_id == "canon-account-a");
    projection.broker_generation = 2;
    projection.projection_id = "fixture:2".to_owned();
    runtime.apply_publication(projection).unwrap();
    assert!(matches!(
        runtime
            .provider_presentation("codex")
            .unwrap()
            .selected_account,
        HostUsageProjectionSelectedAccount::Unavailable {
            canonical_account_id: "canon-account-b"
        }
    ));
    assert!(
        runtime
            .account_inventory(Some("codex"))
            .unwrap()
            .iter()
            .all(|account| !account.selected)
    );

    runtime.set_selected_account("codex", None).unwrap();
    assert_eq!(
        runtime
            .provider_presentation("codex")
            .unwrap()
            .selected_account,
        HostUsageProjectionSelectedAccount::Unselected
    );
}

struct SequencedPublisherExecutor {
    view: FocusedUsageView,
    calls: AtomicUsize,
}

impl UsageProviderExecutor for SequencedPublisherExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            ProviderProbeOutcome::success(self.view.clone())
        } else {
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::ProviderUnavailable,
                message: "Provider response mentions HTTP 429 retry at 999999".to_owned(),
                retry_at_epoch: None,
            }
        }
    }
}

fn publisher_parity_view(now_epoch: i64) -> FocusedUsageView {
    let mut weekly = bucket("Weekly");
    weekly.status_slot = Some(StatusSlot::Weekly);
    weekly.remaining_percent = Some(57);
    weekly.reset_label = Some("Resets in 1h 30m".to_owned());
    weekly.resets_at = Some(now_epoch + 5_400);
    let mut spend = bucket("Extra usage");
    spend.status_slot = Some(StatusSlot::Spend);
    spend.used_money = Some(Money::new(4_520, "USD", 2));
    spend.limit_money = Some(Money::new(10_000, "USD", 2));
    spend.used_label = Some("$45.20".to_owned());
    spend.limit_label = Some("$100.00".to_owned());
    let mut view = view_with_buckets(UsageSnapshotStatus::Fresh, vec![weekly, spend]);
    view.focused_agent = Some("codex".to_owned());
    view.focused_provider = Some("Codex".to_owned());
    view.account.provider_label = "OpenAI / Codex".to_owned();
    view.account.account_label = "work@example.test".to_owned();
    view.account.plan_label = Some("Pro".to_owned());
    view.fetched_at_epoch = now_epoch - 300;
    view.updated_label = "Updated 5m ago".to_owned();
    view.status_bar_label = "OpenAI Weekly: 43% used · 57% left".to_owned();
    view
}

fn seed_broker_catalog(data_dir: &std::path::Path, capability: &UsageAccountCapability) {
    let catalog_revision = "fixture-catalog".to_owned();
    let broker_instance_id = "fixture-broker".to_owned();
    let projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "fixture-broker:0".to_owned(),
        generated_at_epoch: 1_700_000_000,
        discovery_revision: catalog_revision.clone(),
        broker_instance_id: broker_instance_id.clone(),
        broker_generation: 0,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        issues: Vec::new(),
    };
    FileProjectionStateStore::under_data_dir(data_dir)
        .store(&ProjectionStateEnvelope {
            schema_version: ProjectionStateEnvelope::SCHEMA_VERSION,
            projection,
            aliases: Vec::new(),
            catalog_revision,
            catalog: vec![UsageCatalogEntry {
                capability: capability.clone(),
                revision: "fixture-account-revision".to_owned(),
            }],
            retry_deadline_epoch: None,
            success_deadline_epoch: None,
            broker_instance_id,
        })
        .expect("seed durable exact-capability catalog");
}

#[test]
fn broker_publication_matches_capsule_metrics_and_native_presentation() {
    use jackin_console::tui::screens::usage::render_at;
    use jackin_console::tui::state::ManagerState;
    use ratatui::{Terminal, backend::TestBackend};

    let temp = tempfile::tempdir().expect("tempdir");
    let capability = UsageAccountCapability {
        account_id: "canon-account-1".to_owned(),
        surface_id: "codex".to_owned(),
    };
    let now_epoch = 1_700_000_000;
    seed_broker_catalog(temp.path(), &capability);
    let executor = Arc::new(SequencedPublisherExecutor {
        view: publisher_parity_view(now_epoch),
        calls: AtomicUsize::new(0),
    });
    let broker_executor: Arc<dyn UsageProviderExecutor> = executor.clone();
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_path_buf()),
        broker_executor,
    )
    .expect("broker");
    let requested = client
        .refresh(capability.clone(), 0, true)
        .expect("request first generation");
    let first = client
        .join(
            capability.clone(),
            requested.generation,
            Duration::from_secs(5),
        )
        .expect("join first generation");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let capsule_view = first.snapshot.as_ref().expect("last-good view");
    assert_eq!(capsule_view.account.account_label, "work@example.test");

    let projection = client.current_projection().expect("current publication");
    projection.validate().expect("valid broker publication");
    assert_eq!(projection.providers.len(), 1);
    assert_eq!(projection.providers[0].provider_id, "openai");
    assert_eq!(projection.providers[0].accounts.len(), 1);
    let published_account = &projection.providers[0].accounts[0];
    assert_eq!(published_account.canonical_account_id, "canon-account-1");
    assert_eq!(published_account.display_label, "work@example.test");
    assert_eq!(
        published_account.identity_kind,
        UsageIdentityKindV1::UnverifiedHandle
    );
    assert_eq!(published_account.lifecycle, UsageLifecycleV1::Available);
    assert_eq!(
        published_account.freshness.phase,
        UsageFreshnessPhaseV1::Current
    );
    assert!(!published_account.freshness.is_stale);
    assert_eq!(
        published_account.freshness.last_good_at_epoch,
        Some(now_epoch - 300)
    );
    assert_eq!(published_account.freshness.retry_at_epoch, None);
    assert_eq!(published_account.credential_expires_at_epoch, None);

    let capsule_bucket = usage_bucket_presentation(&capsule_view.buckets[0]);
    assert_eq!(capsule_bucket.meter_percent, Some(57));
    assert_eq!(capsule_bucket.display_label, "57% left · Resets in 1h 30m");
    let capsule_detail = usage_detail_presentation(capsule_view);
    assert_eq!(capsule_detail.rows[0].row_id, "plan");
    assert_eq!(capsule_detail.rows[1].row_id, "bucket:0");
    assert_eq!(
        capsule_detail.rows[1].meter_percent,
        capsule_bucket.meter_percent
    );
    let tabs = provider_tabs(&[capsule_view]);
    assert_eq!(tabs.len(), 1);
    assert_eq!(tabs[0].label, "OpenAI / Codex · work@example.test");
    assert_eq!(tabs[0].status_label, capsule_bucket.display_label);

    assert_eq!(published_account.metric_groups.len(), 4);
    assert_eq!(
        published_account.metric_groups[0].kind,
        UsageMetricGroupKindV1::Window
    );
    assert_eq!(published_account.metric_groups[0].label, "Weekly");
    assert_eq!(
        window_remaining_percent(&published_account.metric_groups[0]),
        Some(57)
    );
    assert_eq!(
        published_account.metric_groups[0].phase,
        UsageFreshnessPhaseV1::Current
    );
    assert_eq!(
        published_account.metric_groups[0].reset_at_epoch,
        Some(now_epoch + 5_400)
    );
    assert_eq!(
        published_account.metric_groups[1].kind,
        UsageMetricGroupKindV1::Window
    );
    assert_eq!(
        published_account.metric_groups[2].kind,
        UsageMetricGroupKindV1::SpendCap
    );
    assert_eq!(
        published_account.metric_groups[3].kind,
        UsageMetricGroupKindV1::Plan
    );
    assert_eq!(
        window_remaining_percent(&published_account.metric_groups[2]),
        None
    );

    let mut projection_runtime = HostUsageProjectionRuntime::open(
        projection.clone(),
        HostUsageProjectionConfig::under_data_dir(temp.path()),
    )
    .expect("open presentation runtime");
    let presentation = projection_runtime.provider_presentation("codex").unwrap();
    assert!(matches!(
        presentation.selected_account,
        HostUsageProjectionSelectedAccount::Available {
            canonical_account_id: "canon-account-1",
            ..
        }
    ));
    let glance = presentation.glance_metric_group.expect("weekly glance");
    assert_eq!(glance.kind, UsageMetricGroupKindV1::Window);
    assert_eq!(glance.label, "Weekly");
    assert_eq!(
        window_remaining_percent(glance),
        capsule_bucket.meter_percent
    );
    assert_eq!(presentation.detail_metric_groups.len(), 4);

    let mut screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts.len(), 1);
    assert_eq!(screen.accounts[0].canonical_account_id, "canon-account-1");
    assert_eq!(
        screen.accounts[0].windows[0].meter_percent(),
        capsule_bucket.meter_percent
    );
    assert_eq!(
        screen.accounts[0].metric_groups[0].meter_percent(),
        capsule_bucket.meter_percent
    );
    screen.move_selection(1);
    screen.detail = true;
    let config = jackin_config::AppConfig::default();
    let mut manager = ManagerState::from_config(&config, std::path::Path::new("/test"));
    manager.usage.screen = Some(screen);
    let backend = TestBackend::new(200, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_at(frame, frame.area(), &manager, now_epoch))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rendered = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<Vec<_>>()
                .join("")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("OpenAI / Codex"), "{rendered}");
    assert!(rendered.contains("work@example.test"), "{rendered}");
    assert!(rendered.contains("unverified handle"), "{rendered}");
    assert!(rendered.contains("57% left"), "{rendered}");

    let next_request = client
        .refresh(capability.clone(), first.generation, true)
        .expect("request failed refresh generation");
    let failed = client
        .join(
            capability.clone(),
            next_request.generation,
            Duration::from_secs(5),
        )
        .expect("join failed refresh generation");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
    assert_eq!(failed.phase, UsageRefreshPhase::Failed);
    let failed_projection = client.current_projection().expect("failed publication");
    failed_projection
        .validate()
        .expect("valid failed publication");
    let stale = &failed_projection.providers[0].accounts[0];
    assert_eq!(stale.lifecycle, UsageLifecycleV1::Available);
    assert_eq!(stale.freshness.phase, UsageFreshnessPhaseV1::Stale);
    assert!(stale.freshness.is_stale);
    assert_eq!(stale.freshness.last_good_at_epoch, Some(now_epoch - 300));
    assert!(stale.freshness.retry_at_epoch.is_some());
    assert_ne!(stale.freshness.retry_at_epoch, Some(999_999));
    assert_eq!(stale.issues.len(), 1);
    assert_eq!(stale.issues[0].code, "provider_unavailable");
    assert_eq!(
        stale.issues[0].message,
        "Provider response mentions HTTP 429 retry at 999999"
    );
    projection_runtime
        .apply_publication(failed_projection)
        .unwrap();
    let stale_presentation = projection_runtime.provider_presentation("codex").unwrap();
    assert!(matches!(
        stale_presentation.selected_account,
        HostUsageProjectionSelectedAccount::Available {
            canonical_account_id: "canon-account-1",
            account,
        } if account.freshness.phase == UsageFreshnessPhaseV1::Stale
            && account.freshness.retry_at_epoch.is_some()
            && account.freshness.retry_at_epoch != Some(999_999)
    ));
}
