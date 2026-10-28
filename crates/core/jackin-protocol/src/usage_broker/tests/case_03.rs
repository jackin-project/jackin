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

fn projection_with_windows(windows: Vec<UsageLimitWindowV1>) -> UsageProjectionV1 {
    let mut projection = projection_with_window(projection_window());
    projection.providers[0].accounts[0].windows = windows;
    projection
}

fn unlimited_projection_window() -> UsageLimitWindowV1 {
    let mut window = projection_window();
    window.window_id = "unlimited".into();
    window.rank = 1;
    window.label = "Unlimited".into();
    window.value_label = "Unlimited".into();
    window.remaining_percent = None;
    window.remaining_percent_origin = None;
    window.used_percent = None;
    window.used_percent_origin = None;
    window.meter_percent = None;
    window.meter_side = None;
    window.count_quota = None;
    window.provider_quota_state = Some(ProviderQuotaState::Unlimited);
    window.quota_state = UsageQuotaStateV1::NotApplicable;
    window
}

fn count_only_projection_window() -> UsageLimitWindowV1 {
    UsageLimitWindowV1 {
        window_id: "large-count".into(),
        rank: 2,
        category: UsageWindowCategoryV1::Other,
        label: "Requests".into(),
        value_label: "9007199254740993 requests".into(),
        reset_label: "resets daily".into(),
        remaining_percent: None,
        remaining_percent_origin: None,
        used_percent: None,
        used_percent_origin: None,
        meter_percent: None,
        meter_side: None,
        count_quota: Some(CountQuota::new(
            Some(9_007_199_254_740_993),
            None,
            None,
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
            CountQuotaProvenance::ProviderReported,
        )),
        provider_quota_state: None,
        money_used_percent: None,
        reset_at_epoch: None,
        quota_state: UsageQuotaStateV1::Available,
        pace_label: None,
        runs_out_label: None,
    }
}

fn meter_only_projection_window() -> UsageLimitWindowV1 {
    UsageLimitWindowV1 {
        window_id: "meter-only".into(),
        rank: 3,
        category: UsageWindowCategoryV1::Other,
        label: "Weekly".into(),
        value_label: "57% left".into(),
        reset_label: "Resets 3d".into(),
        remaining_percent: None,
        remaining_percent_origin: None,
        used_percent: None,
        used_percent_origin: None,
        meter_percent: Some(MeterPercentPoints::new(57.0).unwrap()),
        meter_side: Some(UsageMeterSide::Remaining),
        count_quota: None,
        provider_quota_state: None,
        money_used_percent: None,
        reset_at_epoch: None,
        quota_state: UsageQuotaStateV1::Warning,
        pace_label: None,
        runs_out_label: None,
    }
}

fn assert_no_projection_window_geometry(window: &serde_json::Value) {
    for field in [
        "remaining_percent",
        "remaining_percent_origin",
        "used_percent",
        "used_percent_origin",
        "meter_percent",
        "meter_side",
    ] {
        assert!(window.get(field).is_none());
    }
}

#[test]
fn projection_wire_preserves_rich_unlimited_count_and_meter_only_windows() {
    let mut rich = projection_window();
    rich.count_quota = Some(CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    ));

    let unlimited = unlimited_projection_window();
    let count_only = count_only_projection_window();
    let meter_only = meter_only_projection_window();

    let projection = projection_with_windows(vec![
        rich.clone(),
        unlimited.clone(),
        count_only.clone(),
        meter_only.clone(),
    ]);
    projection.validate().unwrap();

    let wire = serde_json::to_value(&projection).unwrap();
    let windows = wire["providers"][0]["accounts"][0]["windows"]
        .as_array()
        .unwrap();
    assert_eq!(windows.len(), 4);

    assert_eq!(windows[0]["rank"], 0);
    assert_eq!(windows[0]["remaining_percent"], 57.125);
    assert_eq!(windows[0]["remaining_percent_origin"], "provider_reported");
    assert_eq!(windows[0]["used_percent"], 43.375);
    assert_eq!(windows[0]["used_percent_origin"], "provider_reported");
    assert_eq!(windows[0]["meter_percent"], 10.0);
    assert_eq!(windows[0]["meter_side"], "remaining");
    assert_eq!(windows[0]["count_quota"]["used"], 9);
    assert_eq!(windows[0]["count_quota"]["limit"], 10);
    assert_eq!(windows[0]["count_quota"]["remaining"], 1);

    assert_eq!(windows[1]["rank"], 1);
    assert_eq!(windows[1]["provider_quota_state"], "unlimited");
    assert_eq!(windows[1]["quota_state"], "not_applicable");
    assert!(windows[1].get("count_quota").is_none());
    assert_no_projection_window_geometry(&windows[1]);

    assert_eq!(windows[2]["rank"], 2);
    assert_eq!(windows[2]["quota_state"], "available");
    assert_eq!(
        windows[2]["count_quota"],
        serde_json::json!({
            "used": 9_007_199_254_740_993_u64,
            "limit": null,
            "remaining": null,
            "unit": "requests",
            "period": "utc_daily",
            "provenance": "provider_reported"
        })
    );
    assert_no_projection_window_geometry(&windows[2]);

    assert_eq!(windows[3]["rank"], 3);
    assert_eq!(windows[3]["label"], "Weekly");
    assert_eq!(windows[3]["value_label"], "57% left");
    assert_eq!(windows[3]["quota_state"], "warning");
    assert_eq!(windows[3]["meter_percent"], 57.0);
    assert_eq!(windows[3]["meter_side"], "remaining");
    for field in [
        "remaining_percent",
        "remaining_percent_origin",
        "used_percent",
        "used_percent_origin",
        "count_quota",
        "provider_quota_state",
        "money_used_percent",
    ] {
        assert!(windows[3].get(field).is_none());
    }
    assert_eq!(
        serde_json::from_value::<UsageProjectionV1>(wire).unwrap(),
        projection
    );

    let mut missing_meter_side = rich;
    missing_meter_side.meter_side = None;
    assert!(
        projection_with_windows(vec![missing_meter_side, unlimited, count_only, meter_only,])
            .validate()
            .unwrap_err()
            .contains("meter geometry and side without a matching pair")
    );
}

#[test]
fn projection_operations_and_publication_response_round_trip() {
    let operations = [
        (
            UsageBrokerOperation::CurrentProjection,
            serde_json::json!({
                "protocol_version": "v9",
                "build_id": "test-build",
                "operation": {
                    "operation": "current_projection"
                }
            }),
        ),
        (
            UsageBrokerOperation::RequestRefresh {
                force: true,
                observed_projection_id: Some("projection-1".into()),
            },
            serde_json::json!({
                "protocol_version": "v9",
                "build_id": "test-build",
                "operation": {
                    "operation": "request_refresh",
                    "force": true,
                    "observed_projection_id": "projection-1"
                }
            }),
        ),
        (
            UsageBrokerOperation::JoinPublication {
                projection_id: "projection-1".into(),
                timeout_ms: 500,
            },
            serde_json::json!({
                "protocol_version": "v9",
                "build_id": "test-build",
                "operation": {
                    "operation": "join_publication",
                    "projection_id": "projection-1",
                    "timeout_ms": 500
                }
            }),
        ),
    ];
    for (operation, wire) in operations {
        let request = UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.into(),
            build_id: "test-build".into(),
            operation,
            launch_credential_scope: None,
        };
        let bytes = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            serde_json::from_slice::<UsageBrokerRequest>(&bytes).unwrap(),
            request
        );
        assert_eq!(serde_json::to_value(&request).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<UsageBrokerRequest>(wire).unwrap(),
            request
        );
    }

    let mut rich = projection_window();
    rich.count_quota = Some(CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    ));
    let unlimited = unlimited_projection_window();
    let count_only = count_only_projection_window();
    let meter_only = meter_only_projection_window();
    let projection = projection_with_windows(vec![rich, unlimited, count_only, meter_only]);
    projection.validate().unwrap();
    let nested_projection = serde_json::to_value(&projection).unwrap();
    let response = UsageBrokerResponse::Projection {
        projection: Box::new(projection),
    };
    let bytes = serde_json::to_vec(&response).unwrap();
    assert_eq!(
        serde_json::from_slice::<UsageBrokerResponse>(&bytes).unwrap(),
        response
    );
    let wire = serde_json::json!({
        "status": "projection",
        "projection": nested_projection
    });
    assert_eq!(serde_json::to_value(&response).unwrap(), wire);
    assert_eq!(
        serde_json::from_value::<UsageBrokerResponse>(wire).unwrap(),
        response
    );
}

#[test]
fn local_source_handle_identity_kind_preserves_wire_tag() {
    let mut expected = projection_with_window(projection_window());
    expected.providers[0].accounts[0].identity_kind = UsageIdentityKindV1::LocalSourceHandle;
    let encoded = serde_json::to_value(&expected).unwrap();
    assert_eq!(
        encoded["providers"][0]["accounts"][0]["identity_kind"],
        "local_source_handle"
    );

    let mut wire = serde_json::to_value(projection_with_window(projection_window())).unwrap();
    wire["providers"][0]["accounts"][0]["identity_kind"] = serde_json::json!("local_source_handle");
    assert_eq!(
        serde_json::from_value::<UsageProjectionV1>(wire).unwrap(),
        expected
    );
}
