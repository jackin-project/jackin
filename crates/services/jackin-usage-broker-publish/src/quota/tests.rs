// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    CanonicalRemainingPercent, CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
    Money, PercentObservationOrigin, ProviderQuotaState, QuotaBucketView, UsageMeterSide,
    UsageSeverity, UsageSnapshotStatus,
};
use jackin_protocol::usage_broker::{UsageMetricValueV1, UsageQuotaStateV1};

use super::{bucket_has_quantity, quota_state_for_bucket};

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
fn count_state_is_exact_and_shared_by_windows_and_groups() {
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
        let bucket = bucket(remaining, severity, UsageSnapshotStatus::Fresh, unit);
        assert_eq!(
            crate::quota_state(&bucket),
            expected,
            "freshness helper disagrees for remaining={remaining:?}, severity={severity:?}"
        );
        assert_eq!(quota_state_for_bucket(&bucket), expected);

        let window = crate::project_window("account", &bucket, 0).expect("window projects");
        assert_eq!(window.quota_state, expected);
        assert_eq!(
            window
                .remaining_percent
                .map(jackin_protocol::control::RawPercentPoints::get),
            Some(0.0)
        );
        assert_eq!(
            window.remaining_percent_origin,
            Some(PercentObservationOrigin::ProviderReported)
        );
        assert_eq!(window.count_quota, bucket.count_quota);

        let group =
            crate::project_window_group("account", &bucket, UsageSnapshotStatus::Fresh, 1_000, 0)
                .expect("window group projects");
        assert_eq!(group.quota_state, expected);
        group.validate(0).expect("group remains valid");
        let UsageMetricValueV1::Window {
            remaining_percent,
            remaining_percent_origin,
            count_quota,
            ..
        } = group.value
        else {
            panic!("window group has window value")
        };
        assert_eq!(
            remaining_percent.map(jackin_protocol::control::RawPercentPoints::get),
            Some(0.0)
        );
        assert_eq!(
            remaining_percent_origin,
            Some(PercentObservationOrigin::ProviderReported)
        );
        assert_eq!(count_quota, bucket.count_quota);
    }
}

#[test]
fn labels_reset_and_empty_count_do_not_establish_normal_quota_availability() {
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
    assert!(!bucket_has_quantity(&label_only));
    assert_eq!(
        quota_state_for_bucket(&label_only),
        UsageQuotaStateV1::Unknown
    );

    let mut empty_count = label_only;
    empty_count.used_label = None;
    empty_count.limit_label = None;
    empty_count.count_quota = Some(CountQuota::new(
        None,
        None,
        None,
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    assert!(!bucket_has_quantity(&empty_count));
    assert_eq!(
        quota_state_for_bucket(&empty_count),
        UsageQuotaStateV1::Unknown
    );
}

#[test]
fn quota_status_gate_precedes_exact_count_state() {
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
        let bucket = bucket(
            Some(0),
            UsageSeverity::Danger,
            status,
            CountQuotaUnit::Requests,
        );
        assert_eq!(crate::quota_state(&bucket), expected);
        assert_eq!(quota_state_for_bucket(&bucket), expected);
    }
}

#[test]
fn exact_money_capacity_overrides_unverified_zero_but_keeps_reported_conflict() {
    for origin in [
        PercentObservationOrigin::Derived,
        PercentObservationOrigin::HistoricalUnknown,
    ] {
        let mut bucket = bucket(
            Some(1),
            UsageSeverity::Normal,
            UsageSnapshotStatus::Fresh,
            CountQuotaUnit::Requests,
        );
        bucket.count_quota = None;
        bucket.remaining_percent_origin = Some(origin);
        bucket.used_money = Some(Money::new(1, "USD", 2));
        bucket.limit_money = Some(Money::new(100, "USD", 2));
        assert_eq!(crate::quota_state(&bucket), UsageQuotaStateV1::Available);
    }

    let mut conflicting = bucket(
        Some(1),
        UsageSeverity::Normal,
        UsageSnapshotStatus::Fresh,
        CountQuotaUnit::Requests,
    );
    conflicting.count_quota = None;
    conflicting.used_money = Some(Money::new(1, "USD", 2));
    conflicting.limit_money = Some(Money::new(100, "USD", 2));
    assert_eq!(
        crate::quota_state(&conflicting),
        UsageQuotaStateV1::Exhausted
    );
    assert_eq!(
        conflicting
            .remaining_percent
            .map(CanonicalRemainingPercent::get),
        Some(0.0)
    );
    assert_eq!(
        conflicting.remaining_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
}

#[test]
fn explicit_provider_state_overrides_count_and_percent_but_keeps_observations() {
    for (state, remaining_count, raw_percent, severity, expected) in [
        (
            ProviderQuotaState::Exhausted,
            Some(1),
            57.125,
            UsageSeverity::Normal,
            UsageQuotaStateV1::Exhausted,
        ),
        (
            ProviderQuotaState::Unlimited,
            Some(0),
            0.0,
            UsageSeverity::Danger,
            UsageQuotaStateV1::NotApplicable,
        ),
    ] {
        let mut bucket = bucket(
            remaining_count,
            severity,
            UsageSnapshotStatus::Fresh,
            CountQuotaUnit::Requests,
        );
        bucket.provider_quota_state = Some(state);
        bucket.remaining_percent =
            Some(CanonicalRemainingPercent::new(raw_percent).expect("valid raw percent"));
        bucket.used_percent = Some(
            jackin_protocol::control::CanonicalUsedPercent::new(150.0).expect("valid used overage"),
        );
        bucket.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);

        assert_eq!(quota_state_for_bucket(&bucket), expected);
        assert!(bucket_has_quantity(&bucket));
        let window = crate::window_for_bucket("account", 0, &bucket)
            .expect("window retains explicit provider state");
        assert_eq!(window.provider_quota_state, Some(state));
        assert_eq!(window.quota_state, expected);
        match state {
            ProviderQuotaState::Exhausted => {
                assert_eq!(
                    window
                        .meter_percent
                        .map(jackin_protocol::control::MeterPercentPoints::get),
                    Some(0.0)
                );
                assert_eq!(window.meter_side, Some(UsageMeterSide::Remaining));
            }
            ProviderQuotaState::Unlimited => {
                assert_eq!(window.meter_percent, None);
                assert_eq!(window.meter_side, None);
            }
        }
        assert_eq!(
            window.count_quota.and_then(|count| count.remaining),
            remaining_count
        );
        assert_eq!(
            window
                .remaining_percent
                .map(jackin_protocol::control::RawPercentPoints::get),
            Some(raw_percent)
        );
        assert_eq!(
            window
                .used_percent
                .map(jackin_protocol::control::RawPercentPoints::get),
            Some(150.0)
        );

        let group = crate::project_window_group("account", &bucket, bucket.status, 1_000, 0)
            .expect("group retains explicit provider state");
        assert_eq!(group.quota_state, expected);
        group.validate(0).expect("projected group validates");
        let UsageMetricValueV1::Window {
            provider_quota_state,
            meter_percent,
            meter_side,
            count_quota,
            remaining_percent,
            used_percent,
            ..
        } = group.value
        else {
            panic!("projected metric is a window")
        };
        assert_eq!(provider_quota_state, Some(state));
        assert_eq!(meter_percent, window.meter_percent);
        assert_eq!(meter_side, window.meter_side);
        assert_eq!(
            count_quota.and_then(|count| count.remaining),
            remaining_count
        );
        assert_eq!(
            remaining_percent.map(jackin_protocol::control::RawPercentPoints::get),
            Some(raw_percent)
        );
        assert_eq!(
            used_percent.map(jackin_protocol::control::RawPercentPoints::get),
            Some(150.0)
        );
    }
}

#[test]
fn lifecycle_status_precedes_explicit_provider_state() {
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
        let mut bucket = bucket(None, UsageSeverity::Danger, status, CountQuotaUnit::Unknown);
        bucket.provider_quota_state = Some(ProviderQuotaState::Exhausted);
        assert_eq!(quota_state_for_bucket(&bucket), expected);
        assert_eq!(crate::spend_quota_state(&bucket), expected);
        let window = crate::window_for_bucket("account", 0, &bucket)
            .expect("lifecycle-gated state window projects");
        assert_eq!(
            window.provider_quota_state,
            Some(ProviderQuotaState::Exhausted)
        );
        assert_eq!(window.quota_state, expected);
        assert_eq!(
            window
                .meter_percent
                .map(jackin_protocol::control::MeterPercentPoints::get),
            Some(0.0)
        );
        assert_eq!(window.meter_side, Some(UsageMeterSide::Remaining));
    }
}

#[test]
fn state_only_provider_claim_is_a_quantitative_observation() {
    for (state, expected) in [
        (ProviderQuotaState::Exhausted, UsageQuotaStateV1::Exhausted),
        (
            ProviderQuotaState::Unlimited,
            UsageQuotaStateV1::NotApplicable,
        ),
    ] {
        let mut bucket = bucket(
            None,
            UsageSeverity::Normal,
            UsageSnapshotStatus::Fresh,
            CountQuotaUnit::Unknown,
        );
        bucket.count_quota = None;
        bucket.remaining_percent = None;
        bucket.remaining_percent_origin = None;
        bucket.provider_quota_state = Some(state);

        assert!(bucket_has_quantity(&bucket));
        let window =
            crate::window_for_bucket("account", 0, &bucket).expect("state-only window is valid");
        assert_eq!(window.provider_quota_state, Some(state));
        assert_eq!(window.quota_state, expected);
        assert!(window.value_label.starts_with(match state {
            ProviderQuotaState::Exhausted => "Exhausted",
            ProviderQuotaState::Unlimited => "Unlimited",
        }));
        match state {
            ProviderQuotaState::Exhausted => {
                assert_eq!(
                    window
                        .meter_percent
                        .map(jackin_protocol::control::MeterPercentPoints::get),
                    Some(0.0)
                );
                assert_eq!(window.meter_side, Some(UsageMeterSide::Remaining));
            }
            ProviderQuotaState::Unlimited => {
                assert_eq!(window.meter_percent, None);
                assert_eq!(window.meter_side, None);
            }
        }
        let group = crate::project_window_group("account", &bucket, bucket.status, 1_000, 0)
            .expect("state-only group is valid");
        group.validate(0).expect("state-only group validates");
    }
}

#[test]
fn stale_provider_state_keeps_stale_phase_and_last_good_time() {
    let mut bucket = bucket(
        Some(1),
        UsageSeverity::Normal,
        UsageSnapshotStatus::Stale,
        CountQuotaUnit::Requests,
    );
    bucket.provider_quota_state = Some(ProviderQuotaState::Exhausted);

    let group = crate::project_window_group("account", &bucket, bucket.status, 1_000, 0)
        .expect("stale state group projects");
    assert_eq!(group.quota_state, UsageQuotaStateV1::Exhausted);
    assert_eq!(
        group.phase,
        jackin_protocol::usage_broker::UsageFreshnessPhaseV1::Stale
    );
    assert!(group.is_stale);
    assert_eq!(group.last_success_at_epoch, Some(1_000));
}
