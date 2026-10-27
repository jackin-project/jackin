// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_protocol::control::{
    CanonicalRemainingPercent, CanonicalUsedPercent, CountQuota, CountQuotaPeriod,
    CountQuotaProvenance, CountQuotaUnit, FocusedUsageView, MeterPercentPoints, Money,
    PercentObservationOrigin, ProviderQuotaState, QuotaBucketView, RawPercentPoints, StatusSlot,
    UsageMeterSide, UsageSeverity, UsageSnapshotStatus,
};

fn bucket() -> QuotaBucketView {
    QuotaBucketView {
        label: "Requests".to_owned(),
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
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}

fn count_quota(
    used: Option<u64>,
    limit: Option<u64>,
    remaining: Option<u64>,
    unit: CountQuotaUnit,
    period: CountQuotaPeriod,
) -> CountQuota {
    CountQuota::new(
        used,
        limit,
        remaining,
        unit,
        period,
        CountQuotaProvenance::ProviderReported,
    )
}

fn set_remaining(bucket: &mut QuotaBucketView, points: f64, origin: PercentObservationOrigin) {
    bucket.remaining_percent =
        Some(CanonicalRemainingPercent::new(points).expect("valid remaining percent"));
    bucket.remaining_percent_origin = Some(origin);
}

fn set_used(bucket: &mut QuotaBucketView, points: f64, origin: PercentObservationOrigin) {
    bucket.used_percent = Some(CanonicalUsedPercent::new(points).expect("valid used percent"));
    bucket.used_percent_origin = Some(origin);
}

fn focused_view(buckets: Vec<QuotaBucketView>) -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("quota-test", 1_800_000_000);
    view.status = UsageSnapshotStatus::Fresh;
    view.buckets = buckets;
    view
}

fn assert_summary_selects(bucket: QuotaBucketView) {
    assert!(crate::summary_bucket(std::slice::from_ref(&bucket)).is_some());
}

#[test]
fn money_percent_uses_each_amount_scale() {
    let mut bucket = bucket();
    bucket.used_money = Some(Money::new(150, "USD", 2));
    bucket.limit_money = Some(Money::new(1, "USD", 0));

    assert_eq!(
        money_used_percent(&bucket).map(RawPercentPoints::get),
        Some(150.0)
    );

    bucket.used_money = Some(Money::new(999_999_999_999_999_999, "USD", 2));
    bucket.limit_money = Some(Money::new(100_000_000_000_000_000, "USD", 0));
    let points = money_used_percent(&bucket)
        .map(RawPercentPoints::get)
        .expect("large mixed-scale ratio remains finite");
    assert!(points.is_finite());
    assert!((points - 10.0).abs() < 1e-12);

    bucket.used_money = Some(Money::new(1, "USD", 0));
    bucket.limit_money = Some(Money::new(i64::MAX, "USD", 255));
    assert!(
        money_used_percent(&bucket)
            .map(RawPercentPoints::get)
            .is_some_and(f64::is_finite)
    );
    bucket.used_money = Some(Money::new(1, "USD", 255));
    bucket.limit_money = Some(Money::new(i64::MAX, "USD", 0));
    assert!(
        money_used_percent(&bucket)
            .map(RawPercentPoints::get)
            .is_some_and(|points| points > 0.0)
    );
}

#[test]
fn public_bucket_geometry_keeps_count_and_reported_inputs_distinct() {
    let mut exact_count = bucket();
    set_remaining(
        &mut exact_count,
        57.125,
        PercentObservationOrigin::ProviderReported,
    );
    exact_count.count_quota = Some(count_quota(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
    ));

    let presentation = crate::usage_bucket_presentation(&exact_count);
    assert_eq!(
        presentation.meter_percent.map(MeterPercentPoints::get),
        Some(10.0)
    );
    assert_eq!(presentation.meter_side, Some(UsageMeterSide::Remaining));
    assert_eq!(presentation.remaining_label.as_deref(), Some("57.13% left"));
    assert!(
        presentation
            .display_segments
            .contains(&"1 requests left".to_owned())
    );
    assert_eq!(
        exact_count
            .remaining_percent
            .map(CanonicalRemainingPercent::get),
        Some(57.125)
    );

    let mut incomplete_count = bucket();
    set_remaining(
        &mut incomplete_count,
        57.125,
        PercentObservationOrigin::ProviderReported,
    );
    incomplete_count.count_quota = Some(count_quota(
        Some(9),
        Some(0),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
    ));

    let presentation = crate::usage_bucket_presentation(&incomplete_count);
    assert_eq!(presentation.meter_percent, None);
    assert_eq!(presentation.meter_side, None);
    assert_eq!(presentation.remaining_label.as_deref(), Some("57.13% left"));
    assert!(
        presentation
            .display_segments
            .contains(&"1 requests left".to_owned())
    );
    assert_eq!(
        incomplete_count
            .remaining_percent
            .map(CanonicalRemainingPercent::get),
        Some(57.125)
    );

    let mut independent_sides = bucket();
    set_used(
        &mut independent_sides,
        43.375,
        PercentObservationOrigin::ProviderReported,
    );
    set_remaining(
        &mut independent_sides,
        12.625,
        PercentObservationOrigin::ProviderReported,
    );
    let presentation = crate::usage_bucket_presentation(&independent_sides);
    assert_eq!(
        presentation.meter_percent.map(MeterPercentPoints::get),
        Some(43.375)
    );
    assert_eq!(presentation.meter_side, Some(UsageMeterSide::Used));
    assert_eq!(
        independent_sides
            .used_percent
            .map(CanonicalUsedPercent::get),
        Some(43.375)
    );
    assert_eq!(
        independent_sides
            .remaining_percent
            .map(CanonicalRemainingPercent::get),
        Some(12.625)
    );

    let mut money_and_provider = bucket();
    money_and_provider.used_money = Some(Money::new(-10, "USD", 2));
    money_and_provider.limit_money = Some(Money::new(100, "USD", 2));
    set_used(
        &mut money_and_provider,
        43.375,
        PercentObservationOrigin::ProviderReported,
    );
    assert_eq!(
        money_used_percent(&money_and_provider).map(RawPercentPoints::get),
        Some(-10.0)
    );
    let presentation = crate::usage_bucket_presentation(&money_and_provider);
    assert_eq!(
        presentation.meter_percent.map(MeterPercentPoints::get),
        Some(43.375)
    );
    assert_eq!(presentation.meter_side, Some(UsageMeterSide::Used));

    let mut historical_zero = bucket();
    historical_zero.used_money = Some(Money::new(1, "USD", 2));
    historical_zero.limit_money = Some(Money::new(100, "USD", 2));
    set_remaining(
        &mut historical_zero,
        0.0,
        PercentObservationOrigin::HistoricalUnknown,
    );
    let presentation = crate::usage_bucket_presentation(&historical_zero);
    assert_eq!(
        presentation.meter_percent.map(MeterPercentPoints::get),
        Some(1.0)
    );
    assert_eq!(presentation.meter_side, Some(UsageMeterSide::Used));
}

#[test]
fn public_rank_keeps_raw_points_separate_from_meter_clamps() {
    assert_fractional_and_overage_ranks_keep_raw_points();

    let mut request_count = bucket();
    request_count.count_quota = Some(count_quota(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
    ));
    set_remaining(
        &mut request_count,
        0.0,
        PercentObservationOrigin::ProviderReported,
    );
    assert_eq!(
        quota_bucket_remaining_rank_points(&request_count),
        Some(10.0)
    );
    let direct_request_rank = quota_source_remaining_rank_points(
        request_count.count_quota,
        None,
        None,
        Some(RawPercentPoints::new(0.0).expect("valid raw percent")),
        Some(PercentObservationOrigin::ProviderReported),
        None,
        None,
    );
    assert_eq!(direct_request_rank, Some(10.0));

    let mut fractional_count = bucket();
    fractional_count.count_quota = Some(count_quota(
        Some(2),
        Some(3),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
    ));
    let rank = quota_bucket_remaining_rank_points(&fractional_count)
        .expect("complete request count has a rank");
    assert!((rank - 100.0 / 3.0).abs() < 0.000_001);
    assert_eq!(
        crate::usage_bucket_presentation(&fractional_count)
            .meter_percent
            .map(MeterPercentPoints::get),
        Some(33.0)
    );

    let mut over_limit_count = bucket();
    over_limit_count.count_quota = Some(count_quota(
        Some(0),
        Some(100),
        Some(101),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
    ));
    assert_eq!(
        quota_bucket_remaining_rank_points(&over_limit_count),
        Some(101.0)
    );
    assert_eq!(
        crate::usage_bucket_presentation(&over_limit_count)
            .meter_percent
            .map(MeterPercentPoints::get),
        Some(100.0)
    );

    let mut unknown_count = bucket();
    unknown_count.count_quota = Some(count_quota(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Unknown,
        CountQuotaPeriod::Unknown,
    ));
    set_remaining(
        &mut unknown_count,
        57.125,
        PercentObservationOrigin::ProviderReported,
    );
    assert_eq!(
        quota_bucket_remaining_rank_points(&unknown_count),
        Some(57.125)
    );
    unknown_count.remaining_percent_origin = Some(PercentObservationOrigin::HistoricalUnknown);
    assert_eq!(quota_bucket_remaining_rank_points(&unknown_count), None);

    let mut money_fallback = bucket();
    money_fallback.used_money = Some(Money::new(-10, "USD", 2));
    money_fallback.limit_money = Some(Money::new(100, "USD", 2));
    set_remaining(
        &mut money_fallback,
        0.0,
        PercentObservationOrigin::HistoricalUnknown,
    );
    assert_eq!(
        quota_bucket_remaining_rank_points(&money_fallback),
        Some(110.0)
    );

    let mut provider_used_precedes_money = bucket();
    provider_used_precedes_money.used_money = Some(Money::new(-10, "USD", 2));
    provider_used_precedes_money.limit_money = Some(Money::new(100, "USD", 2));
    set_used(
        &mut provider_used_precedes_money,
        43.375,
        PercentObservationOrigin::ProviderReported,
    );
    assert_eq!(
        quota_bucket_remaining_rank_points(&provider_used_precedes_money),
        Some(56.625)
    );
}

fn assert_fractional_and_overage_ranks_keep_raw_points() {
    let mut lower_fraction = bucket();
    set_remaining(
        &mut lower_fraction,
        43.374,
        PercentObservationOrigin::ProviderReported,
    );
    let mut higher_fraction = bucket();
    set_remaining(
        &mut higher_fraction,
        43.375,
        PercentObservationOrigin::ProviderReported,
    );
    assert_eq!(
        quota_bucket_remaining_rank_points(&lower_fraction),
        Some(43.374)
    );
    assert_eq!(
        quota_bucket_remaining_rank_points(&higher_fraction),
        Some(43.375)
    );

    let mut fractional_overage = bucket();
    set_used(
        &mut fractional_overage,
        100.125,
        PercentObservationOrigin::ProviderReported,
    );
    let mut large_overage = bucket();
    set_used(
        &mut large_overage,
        150.0,
        PercentObservationOrigin::ProviderReported,
    );
    assert_eq!(
        quota_bucket_remaining_rank_points(&fractional_overage),
        Some(-0.125)
    );
    assert_eq!(
        quota_bucket_remaining_rank_points(&large_overage),
        Some(-50.0)
    );
    for bucket in [&fractional_overage, &large_overage] {
        let presentation = crate::usage_bucket_presentation(bucket);
        assert_eq!(
            presentation.meter_percent.map(MeterPercentPoints::get),
            Some(100.0)
        );
        assert_eq!(presentation.meter_side, Some(UsageMeterSide::Used));
    }
}

#[test]
fn public_summary_selects_state_and_only_typed_observations() {
    assert_summary_selects_typed_candidates_and_state_only();

    let mut used_zero = bucket();
    set_used(
        &mut used_zero,
        0.0,
        PercentObservationOrigin::ProviderReported,
    );
    let mut remaining_zero = bucket();
    set_remaining(
        &mut remaining_zero,
        0.0,
        PercentObservationOrigin::ProviderReported,
    );
    let mut money_limit_only = bucket();
    money_limit_only.limit_money = Some(Money::new(i64::MAX, "USD", 2));

    for candidate in [
        {
            let mut candidate = bucket();
            candidate.count_quota = Some(count_quota(
                Some(0),
                None,
                None,
                CountQuotaUnit::Requests,
                CountQuotaPeriod::UtcDaily,
            ));
            candidate
        },
        {
            let mut candidate = bucket();
            candidate.count_quota = Some(count_quota(
                None,
                Some(u64::MAX),
                None,
                CountQuotaUnit::Requests,
                CountQuotaPeriod::UtcDaily,
            ));
            candidate
        },
        {
            let mut candidate = bucket();
            candidate.count_quota = Some(count_quota(
                None,
                None,
                Some(u64::MAX),
                CountQuotaUnit::Requests,
                CountQuotaPeriod::UtcDaily,
            ));
            candidate
        },
        used_zero,
        remaining_zero,
        money_limit_only,
    ] {
        assert_summary_selects(candidate);
    }

    let mut exhausted = bucket();
    exhausted.provider_quota_state = Some(ProviderQuotaState::Exhausted);
    exhausted.count_quota = Some(count_quota(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
    ));
    set_used(
        &mut exhausted,
        150.0,
        PercentObservationOrigin::ProviderReported,
    );
    set_remaining(
        &mut exhausted,
        57.125,
        PercentObservationOrigin::ProviderReported,
    );
    let exhausted_summary = provider_quota_state_summary(&exhausted).expect("state summary");
    assert!(
        exhausted_summary
            .starts_with("Exhausted · Reported used: 150% · Reported remaining: 57.13%")
    );
    assert!(exhausted_summary.ends_with("1 requests left"));
    assert_eq!(quota_bucket_remaining_rank_points(&exhausted), Some(0.0));
    let exhausted_presentation = crate::usage_bucket_presentation(&exhausted);
    assert_eq!(
        exhausted_presentation
            .meter_percent
            .map(MeterPercentPoints::get),
        Some(0.0)
    );
    assert_eq!(
        exhausted_presentation.meter_side,
        Some(UsageMeterSide::Remaining)
    );
    assert_eq!(
        crate::usage_tab_status_label(&focused_view(vec![exhausted.clone()])),
        exhausted_summary
    );

    let mut unlimited = bucket();
    unlimited.provider_quota_state = Some(ProviderQuotaState::Unlimited);
    unlimited.count_quota = Some(count_quota(
        Some(9),
        Some(10),
        Some(0),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
    ));
    set_used(
        &mut unlimited,
        150.0,
        PercentObservationOrigin::ProviderReported,
    );
    set_remaining(
        &mut unlimited,
        57.125,
        PercentObservationOrigin::ProviderReported,
    );
    let unlimited_summary = provider_quota_state_summary(&unlimited).expect("state summary");
    assert!(unlimited_summary.contains("Reported used: 150%"));
    assert!(unlimited_summary.contains("Reported remaining: 57.13%"));
    assert!(unlimited_summary.ends_with("0 requests left"));
    assert_eq!(quota_bucket_remaining_rank_points(&unlimited), None);
    let unlimited_presentation = crate::usage_bucket_presentation(&unlimited);
    assert_eq!(unlimited_presentation.meter_percent, None);
    assert_eq!(unlimited_presentation.meter_side, None);
    assert_eq!(
        crate::usage_tab_status_label(&focused_view(vec![unlimited.clone()])),
        unlimited_summary
    );
}

fn assert_summary_selects_typed_candidates_and_state_only() {
    let mut label_only = bucket();
    label_only.status_slot = Some(StatusSlot::Daily);
    label_only.used_label = Some("legacy label".to_owned());
    label_only.limit_label = Some("9 requests".to_owned());

    let mut reset_only = bucket();
    reset_only.status_slot = Some(StatusSlot::Weekly);
    reset_only.reset_label = Some("Resets tomorrow".to_owned());
    reset_only.resets_at = Some(1_800_000_100);

    let mut empty_count = bucket();
    empty_count.status_slot = Some(StatusSlot::Daily);
    empty_count.count_quota = Some(count_quota(
        None,
        None,
        None,
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
    ));

    let mut state_only = bucket();
    state_only.provider_quota_state = Some(ProviderQuotaState::Unlimited);
    let candidates = vec![
        label_only.clone(),
        reset_only.clone(),
        empty_count.clone(),
        state_only.clone(),
    ];
    assert_eq!(
        crate::summary_bucket(&candidates).map(|selected| selected.provider_quota_state),
        Some(Some(ProviderQuotaState::Unlimited))
    );
    assert_eq!(
        crate::usage_tab_status_label(&focused_view(candidates)),
        "Unlimited"
    );
    assert_eq!(
        crate::summary_bucket(&[label_only.clone(), reset_only.clone(), empty_count.clone()]),
        None
    );
    assert_eq!(
        crate::usage_tab_status_label(&focused_view(vec![label_only, reset_only, empty_count])),
        "fresh"
    );
}
