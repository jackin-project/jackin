// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_protocol::control::{
    CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit, ProviderQuotaState,
    UsageSource,
};
use jackin_usage_host_accounts::{
    AccountCatalogEntry, AccountLifecycle, AccountProvenance, CanonicalAccountIdentity,
};

use super::*;

fn empty_bucket(label: &str) -> QuotaBucketView {
    QuotaBucketView {
        label: label.to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota: None,
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}

#[test]
fn count_only_glance_keeps_freshness_and_exact_meter_geometry() {
    let count = CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    );
    let mut view = FocusedUsageView::unavailable("fixture", 1_800_000_000);
    view.account.provider_label = "Amp".to_owned();
    view.status = jackin_protocol::control::UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.buckets = vec![QuotaBucketView {
        label: "Daily".to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota: Some(count),
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: Some(jackin_protocol::control::StatusSlot::Daily),
        pace_label: None,
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }];

    let row = build_provider_glance_row(
        HostSurfaceId::Amp,
        &view,
        false,
        1_800_000_000,
        UsageFormatPrefs::default(),
    );
    assert!(view_is_auto_detected(&view));
    assert_eq!(row.status_word, "fresh");
    assert_eq!(row.glance_count_quota, Some(count));
    assert_eq!(
        row.glance_meter_percent,
        Some(MeterPercentPoints::new(10.0).expect("meter"))
    );
    assert_eq!(row.glance_meter_side, Some(UsageMeterSide::Remaining));
    assert_eq!(row.headline, "1 requests left");
    let driving = driving_bucket_from_view(&view).expect("count summary bucket");
    assert_eq!(driving.count_quota, Some(count));
    assert_eq!(driving.meter_percent, row.glance_meter_percent);
    assert_eq!(driving.meter_side, row.glance_meter_side);
}

#[test]
fn glance_preserves_independent_percent_sources_and_used_meter_side() {
    let used = CanonicalUsedPercent::new(43.375).expect("valid used source");
    let remaining = CanonicalRemainingPercent::new(12.625).expect("valid remaining source");
    let mut view = FocusedUsageView::unavailable("fixture", 1_800_000_000);
    view.account.provider_label = "Amp".to_owned();
    view.status = jackin_protocol::control::UsageSnapshotStatus::Fresh;
    view.buckets = vec![QuotaBucketView {
        label: "Daily".to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: Some(used),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        remaining_percent: Some(remaining),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        count_quota: None,
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: Some(jackin_protocol::control::StatusSlot::Daily),
        pace_label: None,
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }];

    let row = build_provider_glance_row(
        HostSurfaceId::Amp,
        &view,
        false,
        1_800_000_000,
        UsageFormatPrefs::default(),
    );
    assert_eq!(row.status_word, "fresh");
    assert_eq!(row.glance_used_percent, Some(used));
    assert_eq!(
        row.glance_used_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(row.glance_remaining_percent, Some(remaining));
    assert_eq!(
        row.glance_remaining_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        row.glance_meter_percent.map(MeterPercentPoints::get),
        Some(43.375)
    );
    assert_eq!(row.glance_meter_side, Some(UsageMeterSide::Used));
    assert_eq!(row.headline, "43.38% used");
}

#[test]
fn quota_ranking_uses_raw_points_when_clamped_meter_values_tie() {
    let used_bucket = |label: &str, value: f64| QuotaBucketView {
        label: label.to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: Some(CanonicalUsedPercent::new(value).expect("valid used percentage")),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota: None,
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    };

    let fractional_overage = used_bucket("fractional", 100.125);
    let larger_overage = used_bucket("larger", 150.0);
    assert_eq!(
        fractional_overage
            .used_percent
            .map(CanonicalUsedPercent::meter_geometry),
        larger_overage
            .used_percent
            .map(CanonicalUsedPercent::meter_geometry)
    );
    assert!(status_bar_rank_key(&larger_overage) < status_bar_rank_key(&fractional_overage));

    for buckets in [
        vec![fractional_overage.clone(), larger_overage.clone()],
        vec![larger_overage.clone(), fractional_overage.clone()],
    ] {
        let mut view = FocusedUsageView::unavailable("fixture", 1_800_000_000);
        view.account.provider_label = "Amp".to_owned();
        view.status = jackin_protocol::control::UsageSnapshotStatus::Fresh;
        view.buckets = buckets;

        let driving = driving_bucket_from_view(&view).expect("overage summary");
        assert_eq!(
            driving.used_percent.map(CanonicalUsedPercent::get),
            Some(150.0)
        );
        assert_eq!(
            driving.meter_percent.map(MeterPercentPoints::get),
            Some(100.0)
        );
    }

    let remaining_bucket = |label: &str, value: f64| QuotaBucketView {
        label: label.to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: Some(
            CanonicalRemainingPercent::new(value).expect("valid remaining percentage"),
        ),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        count_quota: None,
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    };
    let first = remaining_bucket("fractional first", 43.374);
    let second = remaining_bucket("fractional second", 43.375);
    assert!(status_bar_rank_key(&first) < status_bar_rank_key(&second));
}

#[test]
fn state_only_glance_and_account_summary_keep_the_selected_bucket_state() {
    let mut view = FocusedUsageView::unavailable("fixture", 1_800_000_000);
    view.account.account_label = "account@example.test".to_owned();
    view.account.provider_label = "Amp".to_owned();
    view.status = jackin_protocol::control::UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    let mut label_only = empty_bucket("Daily label only");
    label_only.status_slot = Some(jackin_protocol::control::StatusSlot::Daily);
    label_only.used_label = Some("cached text".to_owned());
    let mut reset_only = empty_bucket("Daily reset only");
    reset_only.status_slot = Some(jackin_protocol::control::StatusSlot::Daily);
    reset_only.reset_label = Some("Resets tomorrow".to_owned());
    reset_only.resets_at = Some(1_800_000_100);
    let mut empty_count = empty_bucket("Daily empty count");
    empty_count.status_slot = Some(jackin_protocol::control::StatusSlot::Daily);
    empty_count.count_quota = Some(CountQuota::new(
        None,
        None,
        None,
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    let mut selected = empty_bucket("Session");
    selected.provider_quota_state = Some(ProviderQuotaState::Unlimited);
    selected.status_slot = Some(jackin_protocol::control::StatusSlot::Session);
    let mut other = empty_bucket("Weekly");
    other.status_slot = Some(jackin_protocol::control::StatusSlot::Weekly);
    other.limit_label = Some("10 requests".to_owned());
    view.buckets = vec![label_only, reset_only, empty_count, selected, other];

    assert!(view_is_auto_detected(&view));
    let selected_bucket = glance_bucket(HostSurfaceId::Amp, &view)
        .expect("preferred empty buckets do not hide the state observation");
    assert_eq!(selected_bucket.label, "Session");
    let row = build_provider_glance_row(
        HostSurfaceId::Amp,
        &view,
        false,
        1_800_000_000,
        UsageFormatPrefs::default(),
    );
    assert_eq!(
        row.glance_provider_quota_state,
        Some(ProviderQuotaState::Unlimited)
    );
    assert_eq!(row.glance_meter_percent, None);
    assert_eq!(row.glance_meter_side, None);
    assert_eq!(row.headline, "Unlimited");
    let driving = driving_bucket_from_view(&view).expect("state-only bucket is eligible");
    assert_eq!(
        driving.provider_quota_state,
        Some(ProviderQuotaState::Unlimited)
    );

    let identity = CanonicalAccountIdentity::from_view(HostSurfaceId::Amp, &view)
        .expect("stable account identity");
    let entry = AccountCatalogEntry {
        account_key: identity.account_key(),
        identity,
        account_label: view.account.account_label.clone(),
        username: None,
        plan_label: None,
        provenance: BTreeSet::from([AccountProvenance::LiveHost]),
        discovery_provenance: BTreeSet::new(),
        lifecycle: AccountLifecycle::Current,
        fetched_at_epoch: view.fetched_at_epoch,
        view: view.clone(),
    };
    let descriptor = account_descriptor(
        HostSurfaceId::Amp,
        &entry,
        true,
        1_800_000_000,
        UsageFormatPrefs::default(),
    );
    assert_eq!(
        descriptor.provider_quota_state,
        Some(ProviderQuotaState::Unlimited)
    );
    assert_eq!(descriptor.meter_percent, None);
    assert_eq!(descriptor.meter_side, None);
    assert_eq!(descriptor.headline, "Unlimited");

    let mut insufficient = view.clone();
    insufficient.buckets.truncate(3);
    assert!(!view_is_auto_detected(&insufficient));
    assert_eq!(glance_bucket(HostSurfaceId::Amp, &insufficient), None);
    assert!(driving_bucket_from_view(&insufficient).is_none());
    let mut insufficient_entry = entry.clone();
    insufficient_entry.view = insufficient;
    let empty_descriptor = account_descriptor(
        HostSurfaceId::Amp,
        &insufficient_entry,
        true,
        1_800_000_000,
        UsageFormatPrefs::default(),
    );
    assert_eq!(empty_descriptor.provider_quota_state, None);
    assert_eq!(empty_descriptor.count_quota, None);
    assert_eq!(empty_descriptor.headline, "—");
}

#[test]
fn preferred_empty_slot_does_not_hide_a_lower_priority_count_observation() {
    let mut view = FocusedUsageView::unavailable("fixture", 1_800_000_000);
    view.account.account_label = "account@example.test".to_owned();
    view.account.provider_label = "Amp".to_owned();
    view.status = jackin_protocol::control::UsageSnapshotStatus::Fresh;
    let mut empty_daily = empty_bucket("empty Daily");
    empty_daily.status_slot = Some(jackin_protocol::control::StatusSlot::Daily);
    empty_daily.reset_label = Some("Resets tomorrow".to_owned());
    let mut count_only = empty_bucket("Weekly count");
    count_only.status_slot = Some(jackin_protocol::control::StatusSlot::Weekly);
    count_only.count_quota = Some(CountQuota::new(
        Some(0),
        None,
        None,
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    view.buckets = vec![empty_daily, count_only];

    let selected = glance_bucket(HostSurfaceId::Amp, &view)
        .expect("preferred empty slot falls back to exact count observation");
    assert_eq!(selected.label, "Weekly count");
    assert_eq!(selected.count_quota.and_then(|count| count.used), Some(0));
    let row = build_provider_glance_row(
        HostSurfaceId::Amp,
        &view,
        false,
        1_800_000_000,
        UsageFormatPrefs::default(),
    );
    assert_eq!(row.glance_count_quota.and_then(|count| count.used), Some(0));
    assert_eq!(row.headline, "0 requests used");
}
