// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::control::{CountQuotaProvenance, CountQuotaUnit};

#[test]
fn request_round_trip_preserves_generation_and_force_semantics() {
    let request = UsageBrokerRequest {
        protocol_version: USAGE_BROKER_PROTOCOL_VERSION.into(),
        build_id: "test-build".into(),
        operation: UsageBrokerOperation::Refresh {
            capability: capability(),
            observed_generation: 7,
            force: true,
        },
        launch_credential_scope: None,
    };

    let bytes = serde_json::to_vec(&request).unwrap();
    assert!(bytes.len() < USAGE_BROKER_MAX_FRAME_BYTES);
    assert_eq!(
        serde_json::from_slice::<UsageBrokerRequest>(&bytes).unwrap(),
        request
    );
}

#[test]
fn response_round_trip_keeps_typed_sanitized_failure() {
    let response = UsageBrokerResponse::Error {
        error: UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unauthorized,
            message: "usage account capability is not authorized".into(),
        },
    };

    let bytes = serde_json::to_vec(&response).unwrap();
    assert!(bytes.len() < USAGE_BROKER_MAX_FRAME_BYTES);
    assert_eq!(
        serde_json::from_slice::<UsageBrokerResponse>(&bytes).unwrap(),
        response
    );
}

#[test]
fn broker_conflict_has_a_stable_sanitized_wire_name() {
    let value = serde_json::to_value(UsageCoordinationErrorKind::BrokerConflict)
        .expect("broker conflict should encode");
    assert_eq!(value, "broker_conflict");
    assert_eq!(USAGE_BROKER_PROTOCOL_VERSION, "v9");
}

#[test]
fn local_source_identity_has_its_own_wire_kind() {
    let encoded = serde_json::to_value(UsageIdentityKindV1::LocalSourceHandle)
        .expect("local source identity should encode");
    assert_eq!(encoded, "local_source_handle");
    assert_eq!(
        serde_json::from_value::<UsageIdentityKindV1>(encoded)
            .expect("local source identity should decode"),
        UsageIdentityKindV1::LocalSourceHandle
    );
}

#[test]
fn unverified_identity_has_its_own_wire_kind() {
    let encoded = serde_json::to_value(UsageIdentityKindV1::UnverifiedHandle)
        .expect("unverified identity should encode");
    assert_eq!(encoded, "unverified_handle");
    assert_eq!(
        serde_json::from_value::<UsageIdentityKindV1>(encoded)
            .expect("unverified identity should decode"),
        UsageIdentityKindV1::UnverifiedHandle
    );
}

#[test]
fn projection_operations_and_publication_response_round_trip() {
    let operations = [
        UsageBrokerOperation::CurrentProjection,
        UsageBrokerOperation::RequestRefresh {
            force: true,
            observed_projection_id: Some("projection-1".into()),
        },
        UsageBrokerOperation::JoinPublication {
            projection_id: "projection-1".into(),
            timeout_ms: 500,
        },
    ];
    for operation in operations {
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
    }
    let projection = projection_fixture();
    let response = UsageBrokerResponse::Projection {
        projection: Box::new(projection),
    };
    let bytes = serde_json::to_vec(&response).unwrap();
    assert_eq!(
        serde_json::from_slice::<UsageBrokerResponse>(&bytes).unwrap(),
        response
    );
}

#[test]
fn caller_supplied_catalog_reconciliation_is_not_a_wire_operation() {
    let operation = serde_json::json!({
        "operation": "reconcile_catalog",
        "expected_projection_id": null,
        "catalog_revision": "caller-controlled",
        "entries": [],
    });
    serde_json::from_value::<UsageBrokerOperation>(operation)
        .expect_err("caller-supplied catalog reconciliation must be rejected");
}

#[test]
fn monitor_and_broker_owned_relay_operations_have_v9_wire_shapes() {
    use crate::usage_monitor::{MonitorIssue, MonitorIssueCode, MonitorOperation, MonitorReply};

    let monitor = UsageBrokerOperation::Monitor {
        request: MonitorOperation::ServiceStatus,
    };
    let monitor_value = serde_json::to_value(&monitor).unwrap();
    assert_eq!(monitor_value["operation"], "monitor");
    assert_eq!(monitor_value["request"]["operation"], "service_status");

    let request = UsageBrokerRequest {
        protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
        build_id: "test-build".to_owned(),
        operation: monitor,
        launch_credential_scope: None,
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        serde_json::from_slice::<UsageBrokerRequest>(&bytes).unwrap(),
        request
    );

    let response = UsageBrokerResponse::Monitor {
        reply: MonitorReply::ServiceStopped,
    };
    let response_value = serde_json::to_value(&response).unwrap();
    assert_eq!(response_value["status"], "monitor");
    assert_eq!(response_value["reply"]["result"], "service_stopped");
    assert_eq!(
        serde_json::from_slice::<UsageBrokerResponse>(&serde_json::to_vec(&response).unwrap())
            .unwrap(),
        response
    );

    let relay = UsageBrokerOperation::ResolveRelayCapabilities {
        scope_label: "workspace:research".to_owned(),
        forwarded_sources: UsageRelayForwardedSourcesV1::default(),
    };
    let relay_value = serde_json::to_value(&relay).unwrap();
    assert_eq!(relay_value["operation"], "resolve_relay_capabilities");
    assert_eq!(relay_value["scope_label"], "workspace:research");
    assert_eq!(
        serde_json::from_value::<UsageBrokerOperation>(relay_value).unwrap(),
        relay
    );

    let failure = UsageBrokerResponse::MonitorError {
        issue: MonitorIssue {
            code: MonitorIssueCode::WaitTimeout,
            message: "bounded event wait expired".to_owned(),
            retry_at_epoch: None,
        },
    };
    let failure_value = serde_json::to_value(&failure).unwrap();
    assert_eq!(failure_value["status"], "monitor_error");
    assert_eq!(failure_value["issue"]["code"], "wait_timeout");
    assert_eq!(
        serde_json::from_value::<UsageBrokerResponse>(failure_value).unwrap(),
        failure
    );

    let resolved = UsageBrokerResponse::RelayCapabilities {
        resolution: Box::new(UsageRelayCapabilityResolutionV1::default()),
    };
    let resolved_value = serde_json::to_value(&resolved).unwrap();
    assert_eq!(resolved_value["status"], "relay_capabilities");
    assert_eq!(
        serde_json::from_value::<UsageBrokerResponse>(resolved_value).unwrap(),
        resolved
    );
}

#[test]
fn scoped_capability_request_round_trip_preserves_exact_capability() {
    let request = UsageBrokerRequest {
        protocol_version: USAGE_BROKER_PROTOCOL_VERSION.into(),
        build_id: "test-build".into(),
        operation: UsageBrokerOperation::RefreshForCapability {
            capability: UsageAccountCapability {
                account_id: "account-a".into(),
                surface_id: "claude".into(),
            },
            observed_generation: 3,
            force: false,
        },
        launch_credential_scope: None,
    };

    let bytes = serde_json::to_vec(&request).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("account_id"));
    assert_eq!(
        serde_json::from_slice::<UsageBrokerRequest>(&bytes).unwrap(),
        request
    );
}

#[test]
fn launch_scope_round_trip_contains_identity_and_fingerprint_without_material() {
    let scope = UsageCredentialScope {
        sources: BTreeSet::from([UsageCredentialSourceProof {
            account_id: "account-a".to_owned(),
            surface_id: "zai".to_owned(),
            key: "ZHIPU_API_KEY".to_owned(),
            source: UsageCredentialSourceIdentity::OnePassword {
                reference: "op://vault/item/field".to_owned(),
                account: Some("work".to_owned()),
            },
            material_fingerprint: usage_credential_material_fingerprint("S1"),
        }]),
    };
    let request = UsageBrokerRequest {
        protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
        build_id: "build".to_owned(),
        operation: UsageBrokerOperation::Current {
            capability: capability(),
        },
        launch_credential_scope: Some(scope.clone()),
    };
    let encoded = serde_json::to_vec(&request).unwrap();
    let text = String::from_utf8_lossy(&encoded);
    assert!(!text.contains("S1"));
    assert!(text.contains("op://vault/item/field"));
    assert_eq!(
        serde_json::from_slice::<UsageBrokerRequest>(&encoded).unwrap(),
        request
    );
}

#[test]
fn stdio_tunnel_envelope_round_trips_without_account_metadata() {
    let request = UsageRelayTunnelRequest {
        request_id: 9,
        request: UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: "build".to_owned(),
            operation: UsageBrokerOperation::CurrentForCapability {
                capability: UsageAccountCapability {
                    account_id: "account-a".to_owned(),
                    surface_id: "claude".to_owned(),
                },
            },
            launch_credential_scope: None,
        },
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("account_id"));
    assert_eq!(
        serde_json::from_slice::<UsageRelayTunnelRequest>(&bytes).unwrap(),
        request
    );
}

#[test]
fn canonical_projection_v2_round_trips_historical_fixture_conversion() {
    let fixture = historical_projection_fixture_value();
    let projection: UsageProjectionV1 = serde_json::from_value(fixture.clone()).unwrap();
    projection.validate().unwrap();
    let migrated_window = &projection.providers[0].accounts[0].windows[0];
    assert_eq!(migrated_window.remaining_percent, None);
    assert_eq!(migrated_window.remaining_percent_origin, None);
    assert_eq!(migrated_window.meter_percent.unwrap().get(), 57.0);
    assert_eq!(migrated_window.meter_side, Some(UsageMeterSide::Remaining));
    let encoded = serde_json::to_value(&projection).unwrap();
    assert_eq!(encoded, fixture);
}

#[test]
fn public_projection_accepts_only_schema_two() {
    for version in [1, 3] {
        let error = serde_json::from_value::<UsageProjectionV1>(serde_json::json!({
            "schema_version": version,
            "projection_id": "p",
            "generated_at_epoch": 0,
            "discovery_revision": "d",
            "broker_instance_id": "b",
            "broker_generation": 0,
            "refresh_state": "idle",
            "providers": [],
            "unresolved": [],
            "issues": []
        }))
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported usage projection schema")
        );
    }
}

#[test]
fn projection_requires_origin_pairs_but_allows_independent_percent_sides() {
    let mut projection = projection_fixture();
    let window = &mut projection.providers[0].accounts[0].windows[0];
    window.used_percent_origin = None;
    window.used_percent = Some(RawPercentPoints::new(43.375).unwrap());
    projection.validate().unwrap_err();
    projection.providers[0].accounts[0].windows[0].used_percent_origin =
        Some(PercentObservationOrigin::Derived);
    projection.validate().unwrap();
}

#[test]
fn window_keeps_source_percent_and_count_selected_geometry_separate() {
    let mut projection = projection_fixture();
    {
        let window = &mut projection.providers[0].accounts[0].windows[0];
        window.remaining_percent = Some(RawPercentPoints::new(57.125).unwrap());
        window.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
        window.used_percent = Some(RawPercentPoints::new(43.375).unwrap());
        window.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
        window.count_quota = Some(CountQuota::new(
            Some(9),
            Some(10),
            Some(1),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::Unknown,
            CountQuotaProvenance::ProviderReported,
        ));
        window.meter_percent = Some(MeterPercentPoints::new(10.0).unwrap());
        window.meter_side = Some(UsageMeterSide::Remaining);
    }
    projection.validate().unwrap();
    let window = &projection.providers[0].accounts[0].windows[0];
    assert_eq!(window.remaining_percent.unwrap().get(), 57.125);
    assert_eq!(window.used_percent.unwrap().get(), 43.375);
    assert_eq!(window.meter_percent.unwrap().get(), 10.0);

    let encoded = serde_json::to_value(&projection).unwrap();
    let json_window = &encoded["providers"][0]["accounts"][0]["windows"][0];
    assert_eq!(json_window["remaining_percent"].as_f64(), Some(57.125));
    assert_eq!(json_window["used_percent"].as_f64(), Some(43.375));
    assert_eq!(json_window["count_quota"]["used"].as_u64(), Some(9));
    let decoded: UsageProjectionV1 = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, projection);

    projection.providers[0].accounts[0].windows[0].meter_side = None;
    projection.validate().unwrap_err();
}

#[test]
fn window_metric_preserves_percent_origins_signed_money_ratio_and_count_period() {
    let mut group = window_group("g-window", 0);
    let UsageMetricValueV1::Window {
        remaining_percent,
        remaining_percent_origin,
        used_percent,
        used_percent_origin,
        meter_percent,
        meter_side,
        count_quota,
        provider_quota_state: _,
        money_used_percent,
        period,
        unit,
    } = &mut group.value
    else {
        unreachable!("test helper builds window group")
    };
    *remaining_percent = Some(RawPercentPoints::new(57.0).unwrap());
    *remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    *used_percent = Some(RawPercentPoints::new(43.375).unwrap());
    *used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    *meter_percent = Some(MeterPercentPoints::new(10.0).unwrap());
    *meter_side = Some(UsageMeterSide::Remaining);
    *count_quota = Some(CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    *money_used_percent = Some(RawPercentPoints::new(-10.375).unwrap());
    *period = UsageMetricPeriodV1::Calendar {
        granularity: UsageCalendarPeriodV1::Daily,
    };
    *unit = Some("requests".to_owned());

    group.validate(0).unwrap();
    let encoded = serde_json::to_value(&group).unwrap();
    assert_eq!(encoded["value"]["remaining_percent"].as_f64(), Some(57.0));
    assert_eq!(encoded["value"]["used_percent"].as_f64(), Some(43.375));
    assert_eq!(
        encoded["value"]["money_used_percent"].as_f64(),
        Some(-10.375)
    );
    let decoded: UsageMetricGroupV1 = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, group);

    if let UsageMetricValueV1::Window { period, .. } = &mut group.value {
        *period = UsageMetricPeriodV1::Calendar {
            granularity: UsageCalendarPeriodV1::Weekly,
        };
    }
    assert!(group.validate(0).unwrap_err().contains("count period"));

    let mut unknown = window_group("g-unknown", 0);
    if let UsageMetricValueV1::Window {
        count_quota,
        period,
        unit,
        ..
    } = &mut unknown.value
    {
        *count_quota = Some(CountQuota::new(
            Some(1),
            None,
            None,
            CountQuotaUnit::Unknown,
            CountQuotaPeriod::Unknown,
            CountQuotaProvenance::ProviderReported,
        ));
        *period = UsageMetricPeriodV1::Unknown;
        *unit = None;
    }
    unknown.validate(0).unwrap();
}

#[test]
fn explicit_provider_quota_state_requires_matching_geometry_and_effective_state() {
    let mut projection = projection_fixture();
    let window = &mut projection.providers[0].accounts[0].windows[0];
    window.provider_quota_state = Some(ProviderQuotaState::Exhausted);
    window.quota_state = UsageQuotaStateV1::Exhausted;
    window.meter_percent = Some(MeterPercentPoints::new(0.0).unwrap());
    window.meter_side = Some(UsageMeterSide::Remaining);
    window.count_quota = Some(CountQuota::new(
        Some(9),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    ));
    window.remaining_percent = Some(RawPercentPoints::new(57.125).unwrap());
    window.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    projection.validate().unwrap();

    let mut invalid = projection.clone();
    invalid.providers[0].accounts[0].windows[0].meter_percent = None;
    invalid.providers[0].accounts[0].windows[0].meter_side = None;
    assert!(
        invalid
            .validate()
            .unwrap_err()
            .contains("zero remaining meter")
    );

    let mut invalid = projection.clone();
    invalid.providers[0].accounts[0].windows[0].quota_state = UsageQuotaStateV1::Available;
    assert!(invalid.validate().unwrap_err().contains("inconsistent"));

    // Lifecycle failures remain authoritative while preserving the explicit
    // claim and its selected display geometry.
    let mut unavailable = projection.clone();
    unavailable.providers[0].accounts[0].windows[0].quota_state = UsageQuotaStateV1::Unavailable;
    unavailable.validate().unwrap();

    let mut group = window_group("g-unlimited", 0);
    group.quota_state = UsageQuotaStateV1::NotApplicable;
    if let UsageMetricValueV1::Window {
        provider_quota_state,
        meter_percent,
        meter_side,
        ..
    } = &mut group.value
    {
        *provider_quota_state = Some(ProviderQuotaState::Unlimited);
        *meter_percent = None;
        *meter_side = None;
    }
    group.validate(0).unwrap();

    let mut invalid = group.clone();
    if let UsageMetricValueV1::Window { meter_percent, .. } = &mut invalid.value {
        *meter_percent = Some(MeterPercentPoints::new(99.0).unwrap());
    }
    assert!(invalid.validate(0).unwrap_err().contains("meter geometry"));

    let mut invalid = group.clone();
    invalid.quota_state = UsageQuotaStateV1::Exhausted;
    assert!(invalid.validate(0).unwrap_err().contains("inconsistent"));

    let mut lifecycle_override = group;
    lifecycle_override.quota_state = UsageQuotaStateV1::Error;
    lifecycle_override.validate(0).unwrap();
}

#[test]
fn metric_groups_round_trip_all_six_kinds() {
    let groups = [
        window_group("g-window", 0),
        UsageMetricGroupV1 {
            group_id: "g-balance".into(),
            rank: 1,
            kind: UsageMetricGroupKindV1::Balance,
            label: "Credits".into(),
            quota_state: UsageQuotaStateV1::Available,
            value: UsageMetricValueV1::Balance {
                amount: Money::new(12_50, "USD", 2),
                expires_at_epoch: Some(1_900_000_000),
            },
            reset_at_epoch: None,
            ..window_group("g-balance", 1)
        },
        UsageMetricGroupV1 {
            group_id: "g-spend".into(),
            rank: 2,
            kind: UsageMetricGroupKindV1::SpendCap,
            label: "Extra usage".into(),
            quota_state: UsageQuotaStateV1::Warning,
            value: UsageMetricValueV1::SpendCap {
                cap: Some(Money::new(30_000, "USD", 2)),
                spent: Some(Money::new(27_00, "USD", 2)),
                remaining: Some(Money::new(3_00, "USD", 2)),
            },
            ..window_group("g-spend", 2)
        },
        UsageMetricGroupV1 {
            group_id: "g-tokens".into(),
            rank: 3,
            kind: UsageMetricGroupKindV1::TokenTotals,
            label: "Token totals".into(),
            quota_state: UsageQuotaStateV1::NotApplicable,
            value: UsageMetricValueV1::TokenTotals {
                input: Some(1_000),
                output: Some(2_000),
                cached: None,
                reasoning: Some(50),
                interval_label: Some("current period".into()),
            },
            reset_at_epoch: None,
            ..window_group("g-tokens", 3)
        },
        UsageMetricGroupV1 {
            group_id: "g-rate".into(),
            rank: 4,
            kind: UsageMetricGroupKindV1::RateLimit,
            label: "Requests per minute".into(),
            quota_state: UsageQuotaStateV1::Available,
            value: UsageMetricValueV1::RateLimit {
                limit: Some(60),
                remaining: Some(59),
                window_label: Some("per minute".into()),
            },
            ..window_group("g-rate", 4)
        },
        UsageMetricGroupV1 {
            group_id: "g-plan".into(),
            rank: 5,
            kind: UsageMetricGroupKindV1::Plan,
            label: "Plan".into(),
            quota_state: UsageQuotaStateV1::NotApplicable,
            value: UsageMetricValueV1::Plan {
                plan_label: Some("Pro".into()),
                tier: None,
            },
            reset_at_epoch: None,
            renews_at_epoch: Some(1_900_000_000),
            ..window_group("g-plan", 5)
        },
    ];
    for (rank, group) in groups.iter().enumerate() {
        group.validate(rank).unwrap();
        let bytes = serde_json::to_vec(group).unwrap();
        assert_eq!(
            serde_json::from_slice::<UsageMetricGroupV1>(&bytes).unwrap(),
            group.clone()
        );
    }
}

#[test]
fn metric_group_validate_rejects_mismatched_kind_value_and_money() {
    let mut group = window_group("g", 0);
    group.kind = UsageMetricGroupKindV1::Balance;
    group.validate(0).unwrap_err();
    group.kind = UsageMetricGroupKindV1::Window;

    group.value = UsageMetricValueV1::SpendCap {
        cap: Some(Money::new(1_00, "USD", 2)),
        spent: Some(Money::new(50, "SGD", 2)),
        remaining: None,
    };
    group.kind = UsageMetricGroupKindV1::SpendCap;
    group.validate(0).unwrap_err();

    group.value = UsageMetricValueV1::SpendCap {
        cap: Some(Money::new(1_00, "USD", 2)),
        spent: Some(Money::new(50, "USD", 3)),
        remaining: None,
    };
    group.validate(0).unwrap_err();
}

#[test]
fn metric_group_validate_keeps_reset_renewal_and_issue_scope_separate() {
    let mut group = window_group("g", 0);
    group.renews_at_epoch = Some(1_900_000_000);
    group.validate(0).unwrap_err();
    group.renews_at_epoch = None;

    group.kind = UsageMetricGroupKindV1::Plan;
    group.value = UsageMetricValueV1::Plan {
        plan_label: Some("Pro".into()),
        tier: None,
    };
    group.validate(0).unwrap_err();
    group.reset_at_epoch = None;
    group.renews_at_epoch = Some(1_900_000_000);
    group.validate(0).unwrap();

    group.issues.push(UsageIssueV1 {
        code: "x".into(),
        scope: UsageIssueScopeV1::Account,
        recoverability: UsageIssueRecoverabilityV1::Retryable,
        message: "x".into(),
        retry_at_epoch: None,
    });
    group.validate(0).unwrap_err();
    group.issues[0].scope = UsageIssueScopeV1::Group;
    group.validate(0).unwrap();

    group.rank = 3;
    group.validate(0).unwrap_err();
}
