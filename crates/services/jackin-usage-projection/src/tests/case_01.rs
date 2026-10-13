// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

#[test]
fn parity_antigravity_two_family_windows_groups_and_credits() {
    let (projection, _) = parity_single_provider(ParityProvider {
        provider_id: "antigravity",
        display_name: "Antigravity",
        accounts: vec![parity_antigravity_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts.len(), 1);
    let account = &screen.accounts[0];
    assert_eq!(account.provider, "Antigravity");
    assert_eq!(account.account, "pilot@example.test");
    assert!(!account.unresolved);
    assert_eq!(account.status, "Available");
    assert_eq!(account.lifecycle, UsageLifecycleV1::Available);

    // Principal windows: family membership, order, percents, resets.
    let labels = account
        .windows
        .iter()
        .map(|window| window.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        [
            "Gemini · 5h",
            "Gemini · Weekly",
            "Other models · 5h",
            "Other models · Weekly"
        ]
    );
    let meters = account
        .windows
        .iter()
        .map(UsageWindow::meter_percent)
        .collect::<Vec<_>>();
    assert_eq!(meters, [Some(73), Some(41), Some(12), Some(88)]);
    assert_eq!(account.windows[0].value, "73% left");
    assert_eq!(account.windows[0].reset_at_epoch, Some(PARITY_NOW + 5_430));
    assert_eq!(account.windows[1].reset_at_epoch, Some(PARITY_NOW + 90_000));
    assert!(
        account
            .windows
            .iter()
            .all(|window| window.quota_state == UsageQuotaStateV1::Available)
    );

    // Metric groups: four real window groups, the real plan group, and the
    // hand-built balance group (no producer emits balances yet).
    assert_eq!(account.metric_groups.len(), 6);
    for (index, percent) in [73_u8, 41, 12, 88].iter().enumerate() {
        let group = &account.metric_groups[index];
        assert_eq!(group.kind, UsageMetricGroupKindV1::Window);
        assert_eq!(group.meter_percent(), Some(*percent));
        assert!(
            matches!(
                &group.value,
                UsageMetricValueV1::Window {
                    remaining_percent: Some(remaining),
                    ..
                } if remaining.get() == *percent
            ),
            "window group {index} must carry {percent}% remaining"
        );
    }
    assert_eq!(account.metric_groups[4].kind, UsageMetricGroupKindV1::Plan);
    let balance = &account.metric_groups[5];
    assert_eq!(balance.kind, UsageMetricGroupKindV1::Balance);
    assert_eq!(balance.label, "Credits");
    assert_eq!(balance.scope.pool.as_deref(), Some("credits-pool"));
    assert!(
        matches!(
            &balance.value,
            UsageMetricValueV1::Balance {
                amount,
                expires_at_epoch: Some(expires),
            } if amount == &usd(1_250) && *expires == PARITY_NOW + 2_592_000
        ),
        "balance group must carry $12.50 with expiry"
    );

    // Freshness: same age, renderer-owned words differ (see delta test).
    assert_eq!(freshness_age_label(PARITY_NOW, account), "updated 5m ago");
    assert_eq!(
        group_freshness_label(PARITY_NOW, &account.metric_groups[0]),
        "updated 5m ago"
    );
}
