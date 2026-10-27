// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use crate::{project_window, project_window_group, spend_quota_state, window_for_bucket};
use jackin_protocol::control::{
    CanonicalRemainingPercent, CanonicalUsedPercent, CountQuota, CountQuotaPeriod,
    CountQuotaProvenance, CountQuotaUnit, MeterPercentPoints, Money, PercentObservationOrigin,
    ProviderQuotaState, QuotaBucketView, RawPercentPoints, UsageMeterSide, UsageSeverity,
    UsageSnapshotStatus,
};
use jackin_protocol::usage_broker::{UsageFreshnessPhaseV1, UsageMetricValueV1, UsageQuotaStateV1};

fn bucket(
    remaining: Option<u64>,
    severity: UsageSeverity,
    status: UsageSnapshotStatus,
    unit: CountQuotaUnit,
) -> QuotaBucketView {
    QuotaBucketView {
        label: "Requests".to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: Some(
            CanonicalRemainingPercent::new(0.0).expect("valid raw remaining percent"),
        ),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        count_quota: Some(CountQuota::new(
            Some(9),
            Some(10),
            remaining,
            unit,
            if unit == CountQuotaUnit::Requests {
                CountQuotaPeriod::UtcDaily
            } else {
                CountQuotaPeriod::Unknown
            },
            CountQuotaProvenance::ProviderReported,
        )),
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status,
        used_money: None,
        limit_money: None,
        severity,
    }
}

#[test]
fn money_exhaustion_compares_each_amount_scale_exactly() {
    let mut view = bucket(
        None,
        UsageSeverity::Normal,
        UsageSnapshotStatus::Fresh,
        CountQuotaUnit::Requests,
    );
    view.used_money = Some(Money::new(100, "USD", 2));
    view.limit_money = Some(Money::new(1, "USD", 0));
    assert!(money_is_exhausted(&view));

    view.used_money = Some(Money::new(999_999_999_999_999_999, "USD", 2));
    view.limit_money = Some(Money::new(100_000_000_000_000_000, "USD", 0));
    assert!(!money_is_exhausted(&view));

    view.limit_money = Some(Money::new(0, "USD", 0));
    assert!(!money_is_exhausted(&view));
    view.limit_money = Some(Money::new(1, "SGD", 0));
    assert!(!money_is_exhausted(&view));
}

fn assert_count_projection(bucket: &QuotaBucketView, expected: UsageQuotaStateV1) {
    let window = project_window("account", bucket, 0).expect("count window projects");
    assert_eq!(window.quota_state, expected);
    assert_eq!(
        window.remaining_percent.map(RawPercentPoints::get),
        Some(0.0)
    );
    assert_eq!(
        window.remaining_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(window.count_quota, bucket.count_quota);

    let group = project_window_group("account", bucket, bucket.status, 1_000, 0)
        .expect("count group projects");
    assert_eq!(group.quota_state, expected);
    group.validate(0).expect("count group validates");
    let UsageMetricValueV1::Window {
        remaining_percent,
        remaining_percent_origin,
        count_quota,
        ..
    } = group.value
    else {
        panic!("count group contains a window value")
    };
    assert_eq!(remaining_percent.map(RawPercentPoints::get), Some(0.0));
    assert_eq!(
        remaining_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(count_quota, bucket.count_quota);
}

fn assert_unknown_projection(bucket: &QuotaBucketView) {
    let window = project_window("account", bucket, 0).expect("unknown window projects");
    assert_eq!(window.quota_state, UsageQuotaStateV1::Unknown);
    assert_eq!(window.count_quota, bucket.count_quota);
    assert_eq!(window.remaining_percent, None);
    assert_eq!(window.remaining_percent_origin, None);

    let group = project_window_group("account", bucket, bucket.status, 1_000, 0)
        .expect("unknown group projects");
    assert_eq!(group.quota_state, UsageQuotaStateV1::Unknown);
    group.validate(0).expect("unknown group validates");
    let UsageMetricValueV1::Window {
        remaining_percent,
        remaining_percent_origin,
        count_quota,
        ..
    } = group.value
    else {
        panic!("unknown group contains a window value")
    };
    assert_eq!(remaining_percent, None);
    assert_eq!(remaining_percent_origin, None);
    assert_eq!(count_quota, bucket.count_quota);
}

fn assert_money_origin_projection(
    bucket: &QuotaBucketView,
    expected: UsageQuotaStateV1,
    origin: PercentObservationOrigin,
) {
    let window = project_window("account", bucket, 0).expect("money window projects");
    assert_eq!(window.quota_state, expected);
    assert_eq!(
        window.remaining_percent.map(RawPercentPoints::get),
        Some(0.0)
    );
    assert_eq!(window.remaining_percent_origin, Some(origin));
    assert_eq!(window.count_quota, None);

    let group = project_window_group("account", bucket, bucket.status, 1_000, 0)
        .expect("money group projects");
    assert_eq!(group.quota_state, expected);
    group.validate(0).expect("money group validates");
    let UsageMetricValueV1::Window {
        remaining_percent,
        remaining_percent_origin,
        count_quota,
        ..
    } = group.value
    else {
        panic!("money group contains a window value")
    };
    assert_eq!(remaining_percent.map(RawPercentPoints::get), Some(0.0));
    assert_eq!(remaining_percent_origin, Some(origin));
    assert_eq!(count_quota, None);
}

#[test]
fn public_count_and_money_evidence_preserve_quota_state_and_observations() {
    for (remaining, severity, unit, expected) in [
        (
            Some(0),
            UsageSeverity::Normal,
            CountQuotaUnit::Requests,
            UsageQuotaStateV1::Exhausted,
        ),
        (
            Some(1),
            UsageSeverity::Normal,
            CountQuotaUnit::Requests,
            UsageQuotaStateV1::Available,
        ),
        (
            Some(1),
            UsageSeverity::Danger,
            CountQuotaUnit::Requests,
            UsageQuotaStateV1::Available,
        ),
        (
            Some(1),
            UsageSeverity::Warn,
            CountQuotaUnit::Requests,
            UsageQuotaStateV1::Warning,
        ),
        (
            None,
            UsageSeverity::Danger,
            CountQuotaUnit::Requests,
            UsageQuotaStateV1::Unknown,
        ),
        (
            Some(u64::MAX),
            UsageSeverity::Danger,
            CountQuotaUnit::Requests,
            UsageQuotaStateV1::Available,
        ),
        (
            Some(0),
            UsageSeverity::Normal,
            CountQuotaUnit::Unknown,
            UsageQuotaStateV1::Exhausted,
        ),
        (
            Some(1),
            UsageSeverity::Normal,
            CountQuotaUnit::Unknown,
            UsageQuotaStateV1::Available,
        ),
        (
            None,
            UsageSeverity::Danger,
            CountQuotaUnit::Unknown,
            UsageQuotaStateV1::Unknown,
        ),
    ] {
        let view = bucket(remaining, severity, UsageSnapshotStatus::Fresh, unit);
        assert_count_projection(&view, expected);
    }

    let mut label_only = bucket(
        None,
        UsageSeverity::Normal,
        UsageSnapshotStatus::Fresh,
        CountQuotaUnit::Requests,
    );
    label_only.count_quota = None;
    label_only.remaining_percent = None;
    label_only.remaining_percent_origin = None;
    label_only.used_label = Some("9 requests".to_owned());
    label_only.limit_label = Some("10 requests".to_owned());
    label_only.reset_label = Some("Resets tomorrow".to_owned());
    label_only.resets_at = Some(1_800_000_100);
    assert_unknown_projection(&label_only);
    let label_window = project_window("account", &label_only, 0).expect("label window projects");
    assert_eq!(label_window.reset_label, "Resets tomorrow");
    assert_eq!(label_window.reset_at_epoch, Some(1_800_000_100));

    let mut empty_count = bucket(
        None,
        UsageSeverity::Normal,
        UsageSnapshotStatus::Fresh,
        CountQuotaUnit::Requests,
    );
    empty_count.remaining_percent = None;
    empty_count.remaining_percent_origin = None;
    empty_count.count_quota = Some(CountQuota::new(
        None,
        None,
        None,
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    assert_unknown_projection(&empty_count);

    for (status, expected) in [
        (
            UsageSnapshotStatus::Unavailable,
            UsageQuotaStateV1::Unavailable,
        ),
        (UsageSnapshotStatus::Error, UsageQuotaStateV1::Error),
        (
            UsageSnapshotStatus::NeedsLogin,
            UsageQuotaStateV1::NoPermission,
        ),
    ] {
        let view = bucket(
            Some(0),
            UsageSeverity::Danger,
            status,
            CountQuotaUnit::Requests,
        );
        let window = project_window("account", &view, 0).expect("lifecycle window projects");
        assert_eq!(window.quota_state, expected);
        let group = project_window_group("account", &view, status, 1_000, 0)
            .expect("lifecycle group projects");
        assert_eq!(group.quota_state, expected);
        group.validate(0).expect("lifecycle group validates");
    }

    for (origin, expected) in [
        (
            PercentObservationOrigin::Derived,
            UsageQuotaStateV1::Available,
        ),
        (
            PercentObservationOrigin::HistoricalUnknown,
            UsageQuotaStateV1::Available,
        ),
        (
            PercentObservationOrigin::ProviderReported,
            UsageQuotaStateV1::Exhausted,
        ),
    ] {
        let mut view = bucket(
            Some(1),
            UsageSeverity::Normal,
            UsageSnapshotStatus::Fresh,
            CountQuotaUnit::Requests,
        );
        view.count_quota = None;
        view.remaining_percent_origin = Some(origin);
        view.used_money = Some(Money::new(1, "USD", 2));
        view.limit_money = Some(Money::new(100, "USD", 2));
        assert_money_origin_projection(&view, expected, origin);
    }
}

fn assert_provider_projection(
    bucket: &QuotaBucketView,
    expected: UsageQuotaStateV1,
    expected_meter: Option<(f64, UsageMeterSide)>,
) {
    let (expected_meter_percent, expected_meter_side) =
        expected_meter.map_or((None, None), |(percent, side)| (Some(percent), Some(side)));
    let expected_remaining = bucket.remaining_percent.map(CanonicalRemainingPercent::get);
    let expected_used = bucket.used_percent.map(CanonicalUsedPercent::get);

    let window = window_for_bucket("account", 0, bucket).expect("provider-state window projects");
    assert_eq!(window.quota_state, expected);
    assert_eq!(window.provider_quota_state, bucket.provider_quota_state);
    assert_eq!(
        window.meter_percent.map(MeterPercentPoints::get),
        expected_meter_percent
    );
    assert_eq!(window.meter_side, expected_meter_side);
    assert_eq!(
        window.remaining_percent.map(RawPercentPoints::get),
        expected_remaining
    );
    assert_eq!(
        window.remaining_percent_origin,
        bucket.remaining_percent_origin
    );
    assert_eq!(
        window.used_percent.map(RawPercentPoints::get),
        expected_used
    );
    assert_eq!(window.used_percent_origin, bucket.used_percent_origin);
    assert_eq!(window.count_quota, bucket.count_quota);

    let group = project_window_group("account", bucket, bucket.status, 1_000, 0)
        .expect("provider-state group projects");
    assert_eq!(group.quota_state, expected);
    group.validate(0).expect("provider-state group validates");
    let UsageMetricValueV1::Window {
        remaining_percent,
        remaining_percent_origin,
        used_percent,
        used_percent_origin,
        meter_percent,
        meter_side,
        count_quota,
        provider_quota_state,
        ..
    } = group.value
    else {
        panic!("provider-state group contains a window value")
    };
    assert_eq!(
        meter_percent.map(MeterPercentPoints::get),
        expected_meter_percent
    );
    assert_eq!(meter_side, expected_meter_side);
    assert_eq!(
        remaining_percent.map(RawPercentPoints::get),
        expected_remaining
    );
    assert_eq!(remaining_percent_origin, bucket.remaining_percent_origin);
    assert_eq!(used_percent.map(RawPercentPoints::get), expected_used);
    assert_eq!(used_percent_origin, bucket.used_percent_origin);
    assert_eq!(count_quota, bucket.count_quota);
    assert_eq!(provider_quota_state, bucket.provider_quota_state);
}

fn assert_provider_lifecycle_precedence() {
    for (status, expected) in [
        (UsageSnapshotStatus::Error, UsageQuotaStateV1::Error),
        (
            UsageSnapshotStatus::Unavailable,
            UsageQuotaStateV1::Unavailable,
        ),
        (
            UsageSnapshotStatus::NeedsLogin,
            UsageQuotaStateV1::NoPermission,
        ),
        (
            UsageSnapshotStatus::NeedsSecret,
            UsageQuotaStateV1::NoPermission,
        ),
        (
            UsageSnapshotStatus::Unsupported,
            UsageQuotaStateV1::Unsupported,
        ),
    ] {
        let mut view = bucket(None, UsageSeverity::Danger, status, CountQuotaUnit::Unknown);
        view.provider_quota_state = Some(ProviderQuotaState::Exhausted);
        assert_eq!(spend_quota_state(&view), expected);

        let window = window_for_bucket("account", 0, &view).expect("lifecycle window projects");
        assert_eq!(window.quota_state, expected);
        assert_eq!(
            window.provider_quota_state,
            Some(ProviderQuotaState::Exhausted)
        );
        assert_eq!(window.meter_percent.map(MeterPercentPoints::get), Some(0.0));
        assert_eq!(window.meter_side, Some(UsageMeterSide::Remaining));

        let group = project_window_group("account", &view, status, 1_000, 0)
            .expect("lifecycle group projects");
        assert_eq!(group.quota_state, expected);
        group.validate(0).expect("lifecycle group validates");
        let UsageMetricValueV1::Window {
            provider_quota_state,
            meter_percent,
            meter_side,
            ..
        } = group.value
        else {
            panic!("lifecycle group contains a window value")
        };
        assert_eq!(provider_quota_state, Some(ProviderQuotaState::Exhausted));
        assert_eq!(meter_percent.map(MeterPercentPoints::get), Some(0.0));
        assert_eq!(meter_side, Some(UsageMeterSide::Remaining));
    }
}

fn assert_state_only_provider_projections() {
    for (state, expected, prefix, expected_meter) in [
        (
            ProviderQuotaState::Exhausted,
            UsageQuotaStateV1::Exhausted,
            "Exhausted",
            Some((0.0, UsageMeterSide::Remaining)),
        ),
        (
            ProviderQuotaState::Unlimited,
            UsageQuotaStateV1::NotApplicable,
            "Unlimited",
            None,
        ),
    ] {
        let mut view = bucket(
            None,
            UsageSeverity::Normal,
            UsageSnapshotStatus::Fresh,
            CountQuotaUnit::Unknown,
        );
        view.count_quota = None;
        view.remaining_percent = None;
        view.remaining_percent_origin = None;
        view.provider_quota_state = Some(state);
        assert_provider_projection(&view, expected, expected_meter);

        let window = window_for_bucket("account", 0, &view).expect("state-only window projects");
        assert!(window.value_label.starts_with(prefix));
    }
}

#[test]
fn public_provider_state_lifecycle_and_stale_projection_preserve_observations() {
    for (state, remaining, raw_percent, severity, expected, expected_meter) in [
        (
            ProviderQuotaState::Exhausted,
            Some(1),
            57.125,
            UsageSeverity::Normal,
            UsageQuotaStateV1::Exhausted,
            Some((0.0, UsageMeterSide::Remaining)),
        ),
        (
            ProviderQuotaState::Unlimited,
            Some(0),
            0.0,
            UsageSeverity::Danger,
            UsageQuotaStateV1::NotApplicable,
            None,
        ),
    ] {
        let mut view = bucket(
            remaining,
            severity,
            UsageSnapshotStatus::Fresh,
            CountQuotaUnit::Requests,
        );
        view.provider_quota_state = Some(state);
        view.remaining_percent =
            Some(CanonicalRemainingPercent::new(raw_percent).expect("valid raw percent"));
        view.used_percent = Some(CanonicalUsedPercent::new(150.0).expect("valid used percent"));
        view.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
        assert_provider_projection(&view, expected, expected_meter);
    }

    assert_provider_lifecycle_precedence();
    assert_state_only_provider_projections();

    let mut stale = bucket(
        Some(1),
        UsageSeverity::Normal,
        UsageSnapshotStatus::Stale,
        CountQuotaUnit::Requests,
    );
    stale.provider_quota_state = Some(ProviderQuotaState::Exhausted);
    let group = project_window_group("account", &stale, stale.status, 1_000, 0)
        .expect("stale provider state group projects");
    assert_eq!(group.quota_state, UsageQuotaStateV1::Exhausted);
    assert_eq!(group.phase, UsageFreshnessPhaseV1::Stale);
    assert!(group.is_stale);
    assert_eq!(group.last_success_at_epoch, Some(1_000));
    group
        .validate(0)
        .expect("stale provider state group validates");
}
