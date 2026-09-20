// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_protocol::control::{FocusedAccountHeader, FocusedUsageView, UsageSource};

use super::super::accounts::{AccountLifecycle, AccountProvenance, CanonicalAccountIdentity};
use super::*;

fn sorted(locale: &str, labels: &[&str]) -> Vec<String> {
    let locale = locale.parse::<Locale>().expect("test locale");
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Secondary);
    let collator = Collator::try_new(locale.into(), options).expect("test collator");
    let mut labels = labels.iter().map(ToString::to_string).collect::<Vec<_>>();
    labels.sort_by(|left, right| collator.compare(left, right));
    labels
}

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
        fetched_at_epoch: 1_800_000_000,
        updated_label: "now".into(),
        status_bar_label: "ok".into(),
        tabs: Vec::new(),
        last_error: None,
    }
}

fn catalog_entry(view: FocusedUsageView, plan_label: Option<&str>) -> AccountCatalogEntry {
    AccountCatalogEntry {
        identity: CanonicalAccountIdentity {
            surface: HostSurfaceId::Codex,
            subject: CanonicalAccountSubject::ProviderId("codex-account-1".into()),
        },
        account_key: "codex:default".into(),
        account_label: "work@example.test".into(),
        username: None,
        plan_label: plan_label.map(str::to_owned),
        provenance: BTreeSet::from([AccountProvenance::LiveHost]),
        discovery_provenance: BTreeSet::from(["live".to_owned()]),
        lifecycle: AccountLifecycle::Current,
        view,
        fetched_at_epoch: 1_800_000_000,
    }
}

#[test]
fn window_projection_preserves_raw_overage_from_money_ratio() {
    let mut spend = bucket("Extra usage");
    spend.status_slot = Some(StatusSlot::Spend);
    spend.used_money = Some(Money::new(12_000, "USD", 2));
    spend.limit_money = Some(Money::new(10_000, "USD", 2));
    spend.used_label = Some("$120.00 of $100.00".into());
    let window = project_window("canon-1", &spend, 0).unwrap();
    assert_eq!(window.used_percent.map(UsagePercent::get), Some(100));
    assert_eq!(window.used_raw_percent, Some(120));
    assert_eq!(window.remaining_percent, None);
    assert_eq!(window.quota_state, UsageQuotaStateV1::Exhausted);
    window.validate(0).unwrap();
}

#[test]
fn window_projection_keeps_checked_math_without_wrap_or_fabrication() {
    let mut huge = bucket("Huge");
    huge.used_money = Some(Money::new(i64::MAX, "USD", 2));
    huge.limit_money = Some(Money::new(1, "USD", 2));
    let window = project_window("canon-1", &huge, 0).unwrap();
    assert_eq!(window.used_percent.map(UsagePercent::get), Some(100));
    assert_eq!(window.used_raw_percent, Some(i32::MAX));
    window.validate(0).unwrap();

    let mut mismatched = bucket("Mismatched");
    mismatched.used_money = Some(Money::new(50_00, "USD", 2));
    mismatched.limit_money = Some(Money::new(10_000, "SGD", 2));
    let window = project_window("canon-1", &mismatched, 0).unwrap();
    assert_eq!(window.used_percent, None);
    assert_eq!(window.used_raw_percent, None);
    window.validate(0).unwrap();

    let mut zero_cap = bucket("Zero cap");
    zero_cap.used_money = Some(Money::new(1, "USD", 2));
    zero_cap.limit_money = Some(Money::new(0, "USD", 2));
    let window = project_window("canon-1", &zero_cap, 0).unwrap();
    assert_eq!(window.used_percent, None);
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
fn account_projects_window_spend_and_plan_groups() {
    let mut weekly = bucket("Weekly");
    weekly.remaining_percent = Some(57);
    weekly.resets_at = Some(1_800_100_000);
    weekly.reset_label = Some("Resets soon".into());
    weekly.status_slot = Some(StatusSlot::Weekly);
    let mut spend = bucket("Extra usage");
    spend.status_slot = Some(StatusSlot::Spend);
    spend.used_money = Some(Money::new(27_00, "USD", 2));
    spend.limit_money = Some(Money::new(30_000, "USD", 2));
    spend.used_label = Some("$27.00 of $300.00".into());
    spend.resets_at = Some(1_800_200_000);
    let entry = catalog_entry(
        view_with_buckets(UsageSnapshotStatus::Fresh, vec![weekly, spend]),
        Some("Pro"),
    );

    let account = project_account(&entry, 0, 7).unwrap();
    // Principal-window projection keeps its shape: two buckets, two windows.
    assert_eq!(account.windows.len(), 2);
    assert_eq!(
        account.windows[0].remaining_percent.map(UsagePercent::get),
        Some(57)
    );
    assert_eq!(account.windows[0].remaining_raw_percent, Some(57));

    let kinds = account
        .metric_groups
        .iter()
        .map(|group| group.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::SpendCap,
            UsageMetricGroupKindV1::Plan,
        ]
    );
    for (rank, group) in account.metric_groups.iter().enumerate() {
        group.validate(rank).unwrap();
        assert_eq!(group.fetched_at_epoch, 1_800_000_000);
        assert_eq!(group.observed_at_epoch, Some(1_800_000_000));
        assert_eq!(group.last_success_at_epoch, Some(1_800_000_000));
        assert!(!group.is_stale);
    }
    // Window group mirrors its bucket window.
    assert_eq!(
        account.metric_groups[0].quota_state,
        account.windows[0].quota_state
    );
    assert_eq!(account.metric_groups[0].reset_at_epoch, Some(1_800_100_000));
    assert_eq!(account.metric_groups[0].renews_at_epoch, None);
    // Spend group carries structured money with currency and exponent.
    match &account.metric_groups[2].value {
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
    assert_eq!(account.metric_groups[2].reset_at_epoch, Some(1_800_200_000));
    // Plan group carries metadata with no quota notion and no reset.
    assert_eq!(
        account.metric_groups[3].quota_state,
        UsageQuotaStateV1::NotApplicable
    );
    assert_eq!(account.metric_groups[3].reset_at_epoch, None);
    assert_eq!(account.metric_groups[3].renews_at_epoch, None);

    // Group ids are stable for identical input.
    let rerun = project_account(&entry, 0, 7).unwrap();
    let ids = account
        .metric_groups
        .iter()
        .map(|group| group.group_id.clone())
        .collect::<Vec<_>>();
    let rerun_ids = rerun
        .metric_groups
        .iter()
        .map(|group| group.group_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(ids, rerun_ids);
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

#[test]
fn group_timestamps_track_view_usability() {
    let fresh = catalog_entry(
        view_with_buckets(
            UsageSnapshotStatus::Fresh,
            vec![status_bucket("Weekly", UsageSnapshotStatus::Fresh)],
        ),
        None,
    );
    let account = project_account(&fresh, 0, 1).unwrap();
    assert_eq!(account.metric_groups.len(), 1);
    assert_eq!(
        account.metric_groups[0].phase,
        UsageFreshnessPhaseV1::Current
    );
    assert_eq!(
        account.metric_groups[0].last_success_at_epoch,
        Some(1_800_000_000)
    );

    let stale = catalog_entry(
        view_with_buckets(
            UsageSnapshotStatus::Stale,
            vec![status_bucket("Weekly", UsageSnapshotStatus::Stale)],
        ),
        None,
    );
    let account = project_account(&stale, 0, 1).unwrap();
    assert_eq!(account.metric_groups[0].phase, UsageFreshnessPhaseV1::Stale);
    assert!(account.metric_groups[0].is_stale);
    assert_eq!(
        account.metric_groups[0].last_success_at_epoch,
        Some(1_800_000_000)
    );

    let error = catalog_entry(
        view_with_buckets(
            UsageSnapshotStatus::Error,
            vec![status_bucket("Weekly", UsageSnapshotStatus::Error)],
        ),
        None,
    );
    let account = project_account(&error, 0, 1).unwrap();
    assert_eq!(
        account.metric_groups[0].phase,
        UsageFreshnessPhaseV1::Failed
    );
    assert_eq!(account.metric_groups[0].last_success_at_epoch, None);
    assert_eq!(
        account.metric_groups[0].quota_state,
        UsageQuotaStateV1::Error
    );
}

#[test]
fn credential_expiry_stays_unset_without_provider_signal() {
    let mut weekly = bucket("Weekly");
    weekly.remaining_percent = Some(57);
    weekly.resets_at = Some(1_800_100_000);
    let entry = catalog_entry(
        view_with_buckets(UsageSnapshotStatus::Fresh, vec![weekly]),
        None,
    );
    let account = project_account(&entry, 0, 1).unwrap();
    assert_eq!(account.windows[0].reset_at_epoch, Some(1_800_100_000));
    assert_eq!(account.credential_expires_at_epoch, None);
}

#[test]
fn canonical_projection_icu_collation_goldens_are_pinned() {
    assert_eq!(
        sorted("und", &["Zulu", "Änne", "Ana", "Åke"]),
        ["Åke", "Ana", "Änne", "Zulu"]
    );
    assert_eq!(
        sorted("en", &["Zulu", "Änne", "Ana", "Åke"]),
        ["Åke", "Ana", "Änne", "Zulu"]
    );
    assert_eq!(
        sorted("tr", &["Jale", "İpek", "Işık", "Hale"]),
        ["Hale", "Işık", "İpek", "Jale"]
    );
    assert_eq!(
        sorted("vi", &["Bình", "Ân", "Ăn", "An"]),
        ["An", "Ăn", "Ân", "Bình"]
    );
}
