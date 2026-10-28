// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::control::{CountQuotaProvenance, CountQuotaUnit};

fn counted_window_group(
    group_id: &str,
    count_quota: CountQuota,
    period: UsageMetricPeriodV1,
    unit: Option<&str>,
) -> UsageMetricGroupV1 {
    let mut group = window_group(group_id, 0);
    let UsageMetricValueV1::Window {
        count_quota: value_count,
        period: value_period,
        unit: value_unit,
        ..
    } = &mut group.value
    else {
        unreachable!();
    };
    *value_count = Some(count_quota);
    *value_period = period;
    *value_unit = unit.map(str::to_owned);
    group
}

fn projection_with_window(window: UsageLimitWindowV1) -> UsageProjectionV1 {
    let freshness = UsageFreshnessV1 {
        generation: 1,
        phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(1_800_000_000),
        retry_at_epoch: None,
        is_stale: false,
    };
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV2,
        projection_id: "projection".into(),
        generated_at_epoch: 1_800_000_000,
        discovery_revision: "discovery".into(),
        broker_instance_id: "broker".into(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: vec![UsageProviderV1 {
            provider_id: "provider".into(),
            display_name: "Provider".into(),
            rank: 0,
            membership_state: UsageMembershipStateV1::Current,
            freshness: freshness.clone(),
            accounts: vec![UsageAccountV1 {
                canonical_account_id: "account".into(),
                identity_kind: UsageIdentityKindV1::ProviderAccountId,
                rank: 0,
                display_label: "Account".into(),
                plan_label: None,
                status_label: None,
                lifecycle: UsageLifecycleV1::Available,
                freshness,
                provenance_count: 1,
                windows: vec![window],
                metric_groups: Vec::new(),
                credential_expires_at_epoch: None,
                issues: Vec::new(),
            }],
            issues: Vec::new(),
        }],
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

fn projection_window() -> UsageLimitWindowV1 {
    UsageLimitWindowV1 {
        window_id: "window".into(),
        rank: 0,
        category: UsageWindowCategoryV1::Other,
        label: "Window".into(),
        value_label: "57%".into(),
        reset_label: "resets later".into(),
        remaining_percent: Some(RawPercentPoints::new(57.125).unwrap()),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        used_percent: Some(RawPercentPoints::new(43.375).unwrap()),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        meter_percent: Some(MeterPercentPoints::new(10.0).unwrap()),
        meter_side: Some(UsageMeterSide::Remaining),
        count_quota: None,
        provider_quota_state: None,
        money_used_percent: None,
        reset_at_epoch: None,
        quota_state: UsageQuotaStateV1::Available,
        pace_label: None,
        runs_out_label: None,
    }
}

#[test]
fn metric_group_validation_keeps_metadata_and_count_pairing_separate() {
    let mut group = window_group("metadata", 0);
    group.renews_at_epoch = Some(1_900_000_000);
    assert!(group.validate(0).unwrap_err().contains("renewal timestamp"));

    group.kind = UsageMetricGroupKindV1::Plan;
    group.value = UsageMetricValueV1::Plan {
        plan_label: Some("Pro".into()),
        tier: None,
    };
    assert!(group.validate(0).unwrap_err().contains("reset timestamp"));
    group.reset_at_epoch = None;
    group.validate(0).unwrap();
    group.issues.push(UsageIssueV1 {
        code: "issue".into(),
        scope: UsageIssueScopeV1::Account,
        recoverability: UsageIssueRecoverabilityV1::Retryable,
        message: "retry".into(),
        retry_at_epoch: None,
    });
    assert!(group.validate(0).unwrap_err().contains("non-group issue"));
    group.issues[0].scope = UsageIssueScopeV1::Group;
    group.validate(0).unwrap();
    group.rank = 3;
    assert!(group.validate(0).unwrap_err().contains("noncanonical rank"));

    let requests = CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    );
    let mut daily = counted_window_group(
        "daily-count",
        requests,
        UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Daily,
        },
        Some("requests"),
    );
    daily.validate(0).unwrap();
    if let UsageMetricValueV1::Window { period, .. } = &mut daily.value {
        *period = UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Weekly,
        };
    }
    assert!(daily.validate(0).unwrap_err().contains("count period"));

    let unknown = counted_window_group(
        "unknown-count",
        CountQuota::new(
            Some(1),
            None,
            None,
            CountQuotaUnit::Unknown,
            CountQuotaPeriod::Unknown,
            CountQuotaProvenance::ProviderReported,
        ),
        UsageMetricPeriodV1::Unknown,
        None,
    );
    unknown.validate(0).unwrap();
}

#[test]
fn projection_validation_preserves_origins_and_provider_state_geometry() {
    let mut independent_origins = projection_window();
    let projection = projection_with_window(independent_origins.clone());
    projection.validate().unwrap();
    independent_origins.used_percent_origin = None;
    assert!(
        projection_with_window(independent_origins.clone())
            .validate()
            .unwrap_err()
            .contains("used percent and origin")
    );
    independent_origins.used_percent_origin = Some(PercentObservationOrigin::Derived);
    projection_with_window(independent_origins)
        .validate()
        .unwrap();

    let mut exhausted = projection_window();
    exhausted.provider_quota_state = Some(ProviderQuotaState::Exhausted);
    exhausted.meter_percent = Some(MeterPercentPoints::new(0.0).unwrap());
    exhausted.meter_side = Some(UsageMeterSide::Remaining);
    exhausted.quota_state = UsageQuotaStateV1::Exhausted;
    exhausted.remaining_percent = Some(RawPercentPoints::new(57.125).unwrap());
    exhausted.count_quota = Some(CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    ));
    projection_with_window(exhausted.clone())
        .validate()
        .unwrap();
    let mut missing_meter = exhausted.clone();
    missing_meter.meter_percent = None;
    missing_meter.meter_side = None;
    assert!(
        projection_with_window(missing_meter)
            .validate()
            .unwrap_err()
            .contains("zero remaining meter geometry")
    );
    exhausted.quota_state = UsageQuotaStateV1::Available;
    assert!(
        projection_with_window(exhausted.clone())
            .validate()
            .unwrap_err()
            .contains("inconsistent with an exhausted provider quota")
    );
    exhausted.quota_state = UsageQuotaStateV1::Unavailable;
    projection_with_window(exhausted).validate().unwrap();

    let mut unlimited = window_group("unlimited", 0);
    unlimited.quota_state = UsageQuotaStateV1::NotApplicable;
    if let UsageMetricValueV1::Window {
        provider_quota_state,
        meter_percent,
        meter_side,
        ..
    } = &mut unlimited.value
    {
        *provider_quota_state = Some(ProviderQuotaState::Unlimited);
        *meter_percent = None;
        *meter_side = None;
    }
    unlimited.validate(0).unwrap();

    let mut unlimited_with_meter = unlimited.clone();
    if let UsageMetricValueV1::Window {
        meter_percent,
        meter_side,
        ..
    } = &mut unlimited_with_meter.value
    {
        *meter_percent = Some(MeterPercentPoints::new(10.0).unwrap());
        *meter_side = Some(UsageMeterSide::Remaining);
    }
    assert!(
        unlimited_with_meter
            .validate(0)
            .unwrap_err()
            .contains("meter geometry for an unlimited provider quota")
    );

    unlimited.quota_state = UsageQuotaStateV1::Available;
    assert!(
        unlimited
            .validate(0)
            .unwrap_err()
            .contains("inconsistent with an unlimited provider quota")
    );
    unlimited.quota_state = UsageQuotaStateV1::Error;
    unlimited.validate(0).unwrap();
}

#[test]
fn versioned_window_wire_preserves_signed_money_ratio() {
    let mut group = window_group("money-ratio", 0);
    group.value = UsageMetricValueV1::Window {
        remaining_percent: Some(RawPercentPoints::new(57.0).unwrap()),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        used_percent: Some(RawPercentPoints::new(43.375).unwrap()),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        meter_percent: Some(MeterPercentPoints::new(10.0).unwrap()),
        meter_side: Some(UsageMeterSide::Remaining),
        count_quota: Some(CountQuota::new(
            Some(9),
            Some(10),
            Some(1),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
            CountQuotaProvenance::ProviderReported,
        )),
        provider_quota_state: None,
        money_used_percent: Some(RawPercentPoints::new(-10.375).unwrap()),
        period: UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Daily,
        },
        unit: Some("requests".into()),
    };
    group.validate(0).unwrap();
    let wire = serde_json::to_value(&group).unwrap();
    assert_eq!(wire["value"]["type"], "window");
    assert_eq!(wire["value"]["remaining_percent"], 57.0);
    assert_eq!(wire["value"]["used_percent"], 43.375);
    assert_eq!(wire["value"]["meter_percent"], 10.0);
    assert_eq!(wire["value"]["meter_side"], "remaining");
    assert_eq!(wire["value"]["count_quota"]["used"], 9);
    assert_eq!(wire["value"]["count_quota"]["limit"], 10);
    assert_eq!(wire["value"]["count_quota"]["remaining"], 1);
    assert_eq!(
        wire["value"]["period"],
        serde_json::json!({"period": "calendar", "granularity": "daily"})
    );
    assert_eq!(wire["value"]["unit"], "requests");
    assert_eq!(wire["value"]["money_used_percent"], -10.375);
    assert_eq!(
        serde_json::from_value::<UsageMetricGroupV1>(wire).unwrap(),
        group
    );

    for version in [1, 3] {
        let wire = serde_json::json!({
            "schema_version": version,
            "projection_id": "projection",
            "generated_at_epoch": 1_800_000_000,
            "discovery_revision": "discovery",
            "broker_instance_id": "broker",
            "broker_generation": 1,
            "refresh_state": "idle",
            "providers": [],
            "unresolved": [],
            "issues": []
        });
        assert!(
            serde_json::from_value::<UsageProjectionV1>(wire)
                .unwrap_err()
                .to_string()
                .contains("unsupported usage projection schema version")
        );
    }
}
