// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    CanonicalRemainingPercent, CanonicalUsedPercent, CountQuota, CountQuotaPeriod,
    CountQuotaProvenance, CountQuotaUnit, FocusedUsageView, MeterPercentPoints, Money,
    PercentObservationOrigin, ProviderQuotaState, QuotaBucketView, RawPercentPoints,
    UsageMeterSide, UsageSeverity, UsageSnapshotStatus,
};

use super::{
    money_used_percent, provider_quota_state_summary, quota_bucket_has_observation,
    quota_bucket_remaining_rank_points, quota_source_remaining_rank_points, select_quota_meter,
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

fn remaining(
    value: f64,
    origin: PercentObservationOrigin,
) -> (CanonicalRemainingPercent, PercentObservationOrigin) {
    (
        CanonicalRemainingPercent::new(value).expect("valid remaining percent"),
        origin,
    )
}

#[test]
fn requests_count_selects_exact_geometry_and_keeps_raw_fractional_source() {
    let mut bucket = bucket();
    bucket.remaining_percent =
        Some(remaining(57.125, PercentObservationOrigin::ProviderReported).0);
    bucket.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    bucket.count_quota = Some(CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));

    let (meter, side) = select_quota_meter(&bucket);
    assert_eq!(meter.map(MeterPercentPoints::get), Some(10.0));
    assert_eq!(side, Some(UsageMeterSide::Remaining));
    assert_eq!(
        bucket.remaining_percent.map(CanonicalRemainingPercent::get),
        Some(57.125)
    );
}

#[test]
fn incomplete_requests_count_blocks_raw_percent_fallback() {
    let mut bucket = bucket();
    bucket.remaining_percent =
        Some(remaining(57.125, PercentObservationOrigin::ProviderReported).0);
    bucket.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    bucket.count_quota = Some(CountQuota::new(
        Some(9),
        Some(0),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));

    assert_eq!(select_quota_meter(&bucket), (None, None));
    assert_eq!(
        bucket.remaining_percent.map(CanonicalRemainingPercent::get),
        Some(57.125)
    );
}

#[test]
fn independent_provider_sides_choose_used_without_complementing_either() {
    let mut bucket = bucket();
    bucket.used_percent = Some(CanonicalUsedPercent::new(43.375).expect("valid used percent"));
    bucket.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    bucket.remaining_percent =
        Some(remaining(12.625, PercentObservationOrigin::ProviderReported).0);
    bucket.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);

    assert_eq!(
        select_quota_meter(&bucket).0.map(MeterPercentPoints::get),
        Some(43.375)
    );
    assert_eq!(select_quota_meter(&bucket).1, Some(UsageMeterSide::Used));
    assert_eq!(
        bucket.remaining_percent.map(CanonicalRemainingPercent::get),
        Some(12.625)
    );
}

#[test]
fn exact_signed_money_ratio_and_provider_percent_remain_independent() {
    let mut bucket = bucket();
    bucket.used_money = Some(Money::new(-10, "USD", 2));
    bucket.limit_money = Some(Money::new(100, "USD", 2));
    bucket.used_percent = Some(CanonicalUsedPercent::new(43.375).expect("valid used percent"));
    bucket.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);

    let money = money_used_percent(&bucket).expect("valid signed money ratio");
    assert_eq!(money.get(), -10.0);
    assert_eq!(
        select_quota_meter(&bucket).0.map(MeterPercentPoints::get),
        Some(43.375)
    );
    assert_eq!(select_quota_meter(&bucket).1, Some(UsageMeterSide::Used));
    assert_eq!(
        bucket.used_money.as_ref().map(|amount| amount.amount_minor),
        Some(-10)
    );
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
fn derived_money_percent_does_not_establish_zero_remaining_geometry() {
    let mut bucket = bucket();
    bucket.used_money = Some(Money::new(1, "USD", 2));
    bucket.limit_money = Some(Money::new(100, "USD", 2));
    bucket.remaining_percent = Some(remaining(0.0, PercentObservationOrigin::HistoricalUnknown).0);
    bucket.remaining_percent_origin = Some(PercentObservationOrigin::HistoricalUnknown);

    assert_eq!(
        select_quota_meter(&bucket).0.map(MeterPercentPoints::get),
        Some(1.0)
    );
    assert_eq!(select_quota_meter(&bucket).1, Some(UsageMeterSide::Used));
}

#[test]
fn rank_key_keeps_raw_fraction_and_overage_after_authority_selection() {
    let mut lower_fraction = bucket();
    lower_fraction.remaining_percent =
        Some(remaining(43.374, PercentObservationOrigin::ProviderReported).0);
    lower_fraction.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    let mut higher_fraction = bucket();
    higher_fraction.remaining_percent =
        Some(remaining(43.375, PercentObservationOrigin::ProviderReported).0);
    higher_fraction.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    assert_eq!(
        quota_bucket_remaining_rank_points(&lower_fraction),
        Some(43.374)
    );
    assert_eq!(
        quota_bucket_remaining_rank_points(&higher_fraction),
        Some(43.375)
    );

    let mut fractional_overage = bucket();
    fractional_overage.used_percent =
        Some(CanonicalUsedPercent::new(100.125).expect("valid fractional overage"));
    fractional_overage.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    let mut large_overage = bucket();
    large_overage.used_percent = Some(CanonicalUsedPercent::new(150.0).expect("valid overage"));
    large_overage.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    assert_eq!(
        quota_bucket_remaining_rank_points(&fractional_overage),
        Some(-0.125)
    );
    assert_eq!(
        quota_bucket_remaining_rank_points(&large_overage),
        Some(-50.0)
    );
    assert_eq!(
        select_quota_meter(&fractional_overage)
            .0
            .map(MeterPercentPoints::get),
        Some(100.0)
    );
    assert_eq!(
        select_quota_meter(&large_overage)
            .0
            .map(MeterPercentPoints::get),
        Some(100.0)
    );
}

#[test]
fn broker_rank_key_obeys_count_origin_and_money_authority() {
    let percent = |value| RawPercentPoints::new(value).expect("valid raw points");
    let request_count = CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    );
    assert_eq!(
        quota_source_remaining_rank_points(
            Some(request_count),
            None,
            None,
            Some(percent(0.0)),
            Some(PercentObservationOrigin::ProviderReported),
            None,
            None,
        ),
        Some(10.0)
    );

    let fractional_request_count = CountQuota::new(
        Some(2),
        Some(3),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    );
    let count_rank = quota_source_remaining_rank_points(
        Some(fractional_request_count),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("complete request count has a rank");
    assert!((count_rank - 100.0 / 3.0).abs() < 0.000_001);
    assert_eq!(
        fractional_request_count
            .remaining_meter_percent()
            .map(MeterPercentPoints::get),
        Some(33.0),
        "meter keeps the frozen whole-percent floor while ranking stays raw",
    );

    let over_limit_request_count = CountQuota::new(
        Some(0),
        Some(100),
        Some(101),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    );
    assert_eq!(
        quota_source_remaining_rank_points(
            Some(over_limit_request_count),
            None,
            None,
            None,
            None,
            None,
            None,
        ),
        Some(101.0),
        "rank does not inherit the meter's 100% clamp",
    );
    assert_eq!(
        over_limit_request_count
            .remaining_meter_percent()
            .map(MeterPercentPoints::get),
        Some(100.0),
    );

    let unknown_count = CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Unknown,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    );
    assert_eq!(
        quota_source_remaining_rank_points(
            Some(unknown_count),
            None,
            None,
            Some(percent(57.125)),
            Some(PercentObservationOrigin::ProviderReported),
            None,
            None,
        ),
        Some(57.125)
    );
    assert_eq!(
        quota_source_remaining_rank_points(
            Some(unknown_count),
            None,
            None,
            Some(percent(57.125)),
            Some(PercentObservationOrigin::HistoricalUnknown),
            None,
            None,
        ),
        None
    );

    assert_eq!(
        quota_source_remaining_rank_points(
            None,
            None,
            None,
            Some(percent(0.0)),
            Some(PercentObservationOrigin::HistoricalUnknown),
            Some(percent(-10.0)),
            None,
        ),
        Some(110.0)
    );
}

#[test]
fn explicit_provider_state_precedes_conflicting_geometry_and_rank_inputs() {
    let mut exhausted = bucket();
    exhausted.provider_quota_state = Some(ProviderQuotaState::Exhausted);
    exhausted.count_quota = Some(CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    exhausted.used_percent = Some(CanonicalUsedPercent::new(150.0).expect("valid overage"));
    exhausted.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    exhausted.remaining_percent =
        Some(remaining(57.125, PercentObservationOrigin::ProviderReported).0);
    exhausted.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);

    assert_eq!(
        select_quota_meter(&exhausted),
        (
            Some(MeterPercentPoints::new(0.0).expect("zero geometry")),
            Some(UsageMeterSide::Remaining),
        )
    );
    assert_eq!(quota_bucket_remaining_rank_points(&exhausted), Some(0.0));
    assert!(
        provider_quota_state_summary(&exhausted)
            .expect("state summary")
            .starts_with("Exhausted · Reported used: 150% · Reported remaining: 57.13%")
    );
    let presentation = crate::usage_bucket_presentation(&exhausted);
    assert!(
        presentation
            .display_label
            .starts_with("Exhausted · Reported used: 150%")
    );
    assert_eq!(
        presentation.meter_percent.map(MeterPercentPoints::get),
        Some(0.0)
    );
    assert_eq!(presentation.meter_side, Some(UsageMeterSide::Remaining));
    assert_eq!(
        exhausted.count_quota.and_then(|count| count.remaining),
        Some(1)
    );
    assert_eq!(
        exhausted
            .remaining_percent
            .map(CanonicalRemainingPercent::get),
        Some(57.125)
    );

    let mut unlimited = exhausted;
    unlimited.provider_quota_state = Some(ProviderQuotaState::Unlimited);
    unlimited.count_quota = Some(CountQuota::new(
        Some(9),
        Some(10),
        Some(0),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    unlimited.severity = UsageSeverity::Danger;
    assert_eq!(select_quota_meter(&unlimited), (None, None));
    assert_eq!(quota_bucket_remaining_rank_points(&unlimited), None);
    assert!(
        provider_quota_state_summary(&unlimited)
            .expect("state summary")
            .starts_with("Unlimited · Reported used: 150% · Reported remaining: 57.13%")
    );
    assert_eq!(
        unlimited.count_quota.and_then(|count| count.remaining),
        Some(0)
    );
    assert_eq!(
        unlimited.used_percent.map(CanonicalUsedPercent::get),
        Some(150.0)
    );
    assert_eq!(
        unlimited
            .remaining_percent
            .map(CanonicalRemainingPercent::get),
        Some(57.125)
    );
}

#[test]
fn provider_state_only_is_summarized_and_absent_state_keeps_existing_policy() {
    let mut state_only = bucket();
    state_only.provider_quota_state = Some(ProviderQuotaState::Unlimited);
    assert_eq!(select_quota_meter(&state_only), (None, None));
    assert_eq!(quota_bucket_remaining_rank_points(&state_only), None);
    assert_eq!(
        provider_quota_state_summary(&state_only).as_deref(),
        Some("Unlimited")
    );
    assert!(crate::summary_bucket(&[state_only.clone()]).is_some());

    let mut label_only = bucket();
    label_only.status_slot = Some(jackin_protocol::control::StatusSlot::Daily);
    label_only.used_label = Some("legacy label".to_owned());
    let mut reset_only = bucket();
    reset_only.status_slot = Some(jackin_protocol::control::StatusSlot::Weekly);
    reset_only.reset_label = Some("Resets tomorrow".to_owned());
    reset_only.resets_at = Some(1_800_000_100);
    let mut empty_count = bucket();
    empty_count.status_slot = Some(jackin_protocol::control::StatusSlot::Daily);
    empty_count.count_quota = Some(CountQuota::new(
        None,
        None,
        None,
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    let candidates = vec![label_only, reset_only, empty_count, state_only.clone()];
    assert_eq!(
        crate::summary_bucket(&candidates).map(|selected| selected.provider_quota_state),
        Some(Some(ProviderQuotaState::Unlimited)),
        "non-observation preferred buckets must not hide the state-only source",
    );
    assert_eq!(
        crate::usage_tab_status_label(&FocusedUsageView {
            status: UsageSnapshotStatus::Fresh,
            buckets: vec![state_only.clone()],
            ..FocusedUsageView::unavailable("state-only", 1_800_000_000)
        }),
        "Unlimited"
    );

    assert_eq!(
        crate::summary_bucket(&candidates[..3]),
        None,
        "labels, reset metadata and an empty typed count are not observations",
    );
    let mut insufficient_view = FocusedUsageView::unavailable("insufficient", 1_800_000_000);
    insufficient_view.status = UsageSnapshotStatus::Fresh;
    insufficient_view.buckets = candidates[..3].to_vec();
    assert_eq!(crate::usage_tab_status_label(&insufficient_view), "fresh");

    assert!(quota_bucket_has_observation(&state_only));

    let mut no_claim = bucket();
    no_claim.remaining_percent =
        Some(remaining(57.125, PercentObservationOrigin::ProviderReported).0);
    no_claim.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    assert_eq!(
        select_quota_meter(&no_claim).0.map(MeterPercentPoints::get),
        Some(57.125)
    );
    assert_eq!(quota_bucket_remaining_rank_points(&no_claim), Some(57.125));
    assert_eq!(provider_quota_state_summary(&no_claim), None);
}

#[test]
fn quota_observation_requires_typed_quantity_or_explicit_state() {
    let mut label_only = bucket();
    label_only.used_label = Some("3 / 9 requests".to_owned());
    label_only.limit_label = Some("9 requests".to_owned());
    label_only.reset_label = Some("Resets tomorrow".to_owned());
    label_only.resets_at = Some(1_800_000_100);
    assert!(!quota_bucket_has_observation(&label_only));

    let mut empty_count = bucket();
    empty_count.count_quota = Some(CountQuota::new(
        None,
        None,
        None,
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    assert!(!quota_bucket_has_observation(&empty_count));

    let count_observations = [
        CountQuota::new(
            Some(0),
            None,
            None,
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
            CountQuotaProvenance::ProviderReported,
        ),
        CountQuota::new(
            None,
            Some(u64::MAX),
            None,
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
            CountQuotaProvenance::ProviderReported,
        ),
        CountQuota::new(
            None,
            None,
            Some(u64::MAX),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
            CountQuotaProvenance::ProviderReported,
        ),
    ];
    for count in count_observations {
        let mut observed = bucket();
        observed.count_quota = Some(count);
        assert!(quota_bucket_has_observation(&observed));
    }

    let mut raw_zero = bucket();
    raw_zero.used_percent =
        Some(CanonicalUsedPercent::new(0.0).expect("zero is a valid reported source"));
    raw_zero.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    assert!(quota_bucket_has_observation(&raw_zero));

    let mut remaining_zero = bucket();
    remaining_zero.remaining_percent =
        Some(remaining(0.0, PercentObservationOrigin::ProviderReported).0);
    remaining_zero.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    assert!(quota_bucket_has_observation(&remaining_zero));

    let mut money_only = bucket();
    money_only.limit_money = Some(Money::new(i64::MAX, "USD", 2));
    assert!(quota_bucket_has_observation(&money_only));
}
