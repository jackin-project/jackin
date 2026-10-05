use super::*;
use crate::control::Money;

fn capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "opaque-account".into(),
        surface_id: "claude".into(),
    }
}

#[test]
fn projection_rejects_ambiguous_account_and_refresh_authority() {
    let mut projection: UsageProjectionV2 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v2-current.json"
    ))
    .unwrap();
    projection.providers.truncate(1);
    projection.providers[0].accounts.truncate(1);
    let route = UsageAccountCapability {
        account_id: "revision-scoped-route".into(),
        surface_id: "codex".into(),
    };
    projection.providers[0].accounts[0].refresh_capabilities = vec![route.clone()];
    projection.validate().unwrap();
    projection.providers[0].accounts[0]
        .refresh_capabilities
        .push(route.clone());
    assert!(
        projection
            .validate()
            .unwrap_err()
            .contains("shared refresh capability")
    );
    projection.providers[0].accounts[0]
        .refresh_capabilities
        .pop();
    let mut second = projection.providers[0].accounts[0].clone();
    second.rank = 1;
    projection.providers[0].accounts.push(second);
    assert!(
        projection
            .validate()
            .unwrap_err()
            .contains("duplicate canonical account")
    );
    projection.providers[0].accounts[1].canonical_account_id = "other-logical-account".into();
    assert!(
        projection
            .validate()
            .unwrap_err()
            .contains("shared refresh capability")
    );
    projection.providers[0].accounts.pop();
    projection.providers[0].accounts[0].refresh_capabilities[0].surface_id = "claude".into();
    assert!(projection.validate().unwrap_err().contains("mismatched"));
    projection.providers[0].accounts[0]
        .refresh_capabilities
        .clear();
    projection.validate().unwrap();
    let mut duplicate_provider = projection.providers[0].clone();
    duplicate_provider.rank = 1;
    projection.providers.push(duplicate_provider);
    assert!(
        projection
            .validate()
            .unwrap_err()
            .contains("duplicate provider")
    );
}

#[test]
fn typed_logical_identity_survives_diagnostic_view_round_trip() {
    use crate::control::{UsageCanonicalAccountIdentity, UsageCanonicalAccountSubject};
    for subject in [
        UsageCanonicalAccountSubject::ProviderId("provider-1".into()),
        UsageCanonicalAccountSubject::ProviderStableHandle("authenticated@example.test".into()),
        UsageCanonicalAccountSubject::SourceCapability("source-opaque".into()),
    ] {
        let identity = UsageCanonicalAccountIdentity {
            surface_id: "codex".into(),
            subject,
        };
        let mut view = FocusedUsageView::unavailable("credentials expired", 42);
        view.canonical_identity = Some(identity.clone());
        view.account_identity = Some((&capability()).into());
        let entry = UsageCatalogEntry {
            provenance_count: 1,
            capability: capability(),
            canonical_identity: Some(identity),
            revision: "revision-2".into(),
        };
        assert_eq!(
            serde_json::from_slice::<FocusedUsageView>(&serde_json::to_vec(&view).unwrap())
                .unwrap(),
            view
        );
        assert_eq!(
            serde_json::from_slice::<UsageCatalogEntry>(&serde_json::to_vec(&entry).unwrap())
                .unwrap(),
            entry
        );
    }
}

#[test]
fn canonical_identity_rejects_blank_surface_and_every_blank_subject_kind() {
    use crate::control::{UsageCanonicalAccountIdentity, UsageCanonicalAccountSubject};
    for subject in [
        UsageCanonicalAccountSubject::ProviderId(" \t".into()),
        UsageCanonicalAccountSubject::ProviderStableHandle("\n".into()),
        UsageCanonicalAccountSubject::SourceCapability(String::new()),
    ] {
        let identity = UsageCanonicalAccountIdentity {
            surface_id: "codex".into(),
            subject,
        };
        assert!(identity.validate().unwrap_err().contains("empty subject"));
    }
    let mut identity = UsageCanonicalAccountIdentity {
        surface_id: " \t".into(),
        subject: UsageCanonicalAccountSubject::ProviderId("provider-id".into()),
    };
    assert!(identity.validate().unwrap_err().contains("empty surface"));
    identity.surface_id = "codex".into();
    identity.validate().unwrap();
    identity.subject = UsageCanonicalAccountSubject::ProviderStableHandle(" Exact Handle ".into());
    identity.validate().unwrap();
    assert_eq!(
        identity.subject,
        UsageCanonicalAccountSubject::ProviderStableHandle(" Exact Handle ".into())
    );
}

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
fn projection_operations_and_publication_response_round_trip() {
    let operations = [
        UsageBrokerOperation::ReconcileCatalog {
            expected_projection_id: None,
            catalog_revision: "catalog-2".into(),
            entries: vec![UsageCatalogEntry {
                provenance_count: 1,
                capability: capability(),
                canonical_identity: None,
                revision: "credential-2".into(),
            }],
        },
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
    let projection: UsageProjectionV2 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v2-current.json"
    ))
    .unwrap();
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
fn scoped_capability_request_round_trip_preserves_exact_capability() {
    let request = UsageBrokerRequest {
        protocol_version: USAGE_BROKER_PROTOCOL_VERSION.into(),
        build_id: "test-build".into(),
        operation: UsageBrokerOperation::RefreshForCapability {
            instance_id: "test-instance".to_owned(),
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
fn scoped_operations_require_explicit_instance_selector() {
    for operation in [
        "current_for_capability",
        "refresh_for_capability",
        "join_for_capability",
    ] {
        let mut value = serde_json::json!({
            "operation": operation,
            "capability": {"account_id": "account-a", "surface_id": "claude"},
            "observed_generation": 3,
            "force": false,
            "generation": 3,
            "timeout_ms": 50,
        });
        assert!(serde_json::from_value::<UsageBrokerOperation>(value.clone()).is_err());
        value["instance_id"] = serde_json::json!("test-instance");
        let decoded = serde_json::from_value::<UsageBrokerOperation>(value).unwrap();
        let encoded = serde_json::to_value(decoded).unwrap();
        assert_eq!(encoded["instance_id"], "test-instance");
    }
}

#[test]
fn launch_scope_round_trip_contains_identity_and_fingerprint_without_material() {
    let scope = UsageCredentialScope {
        profiles: BTreeSet::new(),
        sources: BTreeSet::from([UsageCredentialSourceProof {
            instance_id: "instance-fixture".to_owned(),
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
        instance_id: Some("test-instance".to_owned()),
        expires_at_unix_ms: u64::MAX,
        request_id: 9,
        request: UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: "build".to_owned(),
            operation: UsageBrokerOperation::CurrentForCapability {
                instance_id: "test-instance".to_owned(),
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
fn tunnel_route_requires_explicit_instance_or_supervisor_null() {
    let request = UsageRelayTunnelRequest {
        instance_id: None,
        expires_at_unix_ms: u64::MAX,
        request_id: 10,
        request: UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: "build".to_owned(),
            operation: UsageBrokerOperation::CurrentProjectionForSurface,
            launch_credential_scope: None,
        },
    };
    let mut encoded = serde_json::to_value(&request).unwrap();
    assert!(encoded.get("instance_id").unwrap().is_null());
    assert_eq!(
        serde_json::from_value::<UsageRelayTunnelRequest>(encoded.clone()).unwrap(),
        request
    );
    encoded.as_object_mut().unwrap().remove("instance_id");
    assert!(serde_json::from_value::<UsageRelayTunnelRequest>(encoded).is_err());
}

#[test]
fn canonical_projection_v2_round_trips_frozen_fixture() {
    let fixture = include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v2-current.json"
    );
    let projection: UsageProjectionV2 = serde_json::from_str(fixture).unwrap();
    projection.validate().unwrap();
    let encoded = serde_json::to_value(&projection).unwrap();
    let original: serde_json::Value = serde_json::from_str(fixture).unwrap();
    assert_eq!(encoded, original);
}

#[test]
fn canonical_projection_v2_rejects_legacy_v1_and_unknown_major() {
    for schema_version in [1, 3] {
        let encoded = format!(
            r#"{{"schema_version":{schema_version},"projection_id":"p","generated_at_epoch":0,"discovery_revision":"d","broker_instance_id":"b","broker_generation":0,"refresh_state":"idle","providers":[],"unresolved":[],"unresolved_grants":[],"issues":[]}}"#,
        );
        let error = serde_json::from_str::<UsageProjectionV2>(&encoded).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported usage projection schema")
        );
    }
}

#[test]
fn canonical_projection_v2_rejects_invalid_percent_and_cross_field_shape() {
    serde_json::from_str::<UsagePercent>("101").unwrap_err();
    let mut projection: UsageProjectionV2 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v2-current.json"
    ))
    .unwrap();
    let window = &mut projection.providers[0].accounts[0].windows[0];
    window.used_percent = window.remaining_percent;
    projection.validate().unwrap_err();
}

fn window_group(group_id: &str, rank: u32) -> UsageMetricGroupV2 {
    UsageMetricGroupV2 {
        group_id: group_id.into(),
        rank,
        kind: UsageMetricGroupKindV2::Window,
        label: "Weekly".into(),
        scope: UsageMetricScopeV2::default(),
        observed_at_epoch: Some(1_800_000_000),
        fetched_at_epoch: 1_800_000_001,
        last_success_at_epoch: Some(1_800_000_000),
        phase: UsageFreshnessPhaseV2::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV2::Available,
        value: UsageMetricValueV2::Window {
            count_quota: None,
            remaining_percent: Some(UsagePercent::new(57).unwrap()),
            remaining_raw_percent: Some(57),
            used_percent: None,
            used_raw_percent: None,
            period: UsageMetricPeriodV2::Calendar {
                granularity: UsageCalendarPeriodV2::Weekly,
            },
            unit: None,
        },
        reset_at_epoch: Some(1_800_100_000),
        renews_at_epoch: None,
        issues: Vec::new(),
    }
}

#[test]
fn usage_percent_clamp_never_wraps_or_underflows() {
    assert_eq!(UsagePercent::clamp_raw(-5).get(), 0);
    assert_eq!(UsagePercent::clamp_raw(i32::MIN).get(), 0);
    assert_eq!(UsagePercent::clamp_raw(0).get(), 0);
    assert_eq!(UsagePercent::clamp_raw(57).get(), 57);
    assert_eq!(UsagePercent::clamp_raw(100).get(), 100);
    assert_eq!(UsagePercent::clamp_raw(120).get(), 100);
    assert_eq!(UsagePercent::clamp_raw(i32::MAX).get(), 100);
    let (raw, clamped) = UsagePercent::split_raw(120);
    assert_eq!((raw, clamped.get()), (120, 100));
    assert_eq!(UsagePercent::new(100).unwrap().meter_fill(), 100);
}

#[test]
fn window_preserves_overage_raw_beside_clamped_geometry() {
    let mut projection: UsageProjectionV2 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v2-current.json"
    ))
    .unwrap();
    {
        let window = &mut projection.providers[0].accounts[0].windows[0];
        window.remaining_percent = None;
        window.remaining_raw_percent = None;
        window.used_percent = Some(UsagePercent::clamp_raw(120));
        window.used_raw_percent = Some(120);
        assert_eq!(window.used_percent.unwrap().meter_fill(), 100);
        assert_eq!(window.used_raw_percent, Some(120));
    }
    projection.validate().unwrap();

    projection.providers[0].accounts[0].windows[0].used_percent =
        Some(UsagePercent::new(57).unwrap());
    projection.validate().unwrap_err();

    projection.providers[0].accounts[0].windows[0].used_percent = None;
    projection.validate().unwrap_err();

    projection.providers[0].accounts[0].windows[0].used_percent =
        Some(UsagePercent::clamp_raw(120));
    projection.providers[0].accounts[0].windows[0].remaining_percent =
        Some(UsagePercent::new(10).unwrap());
    projection.validate().unwrap_err();
}

#[test]
fn metric_groups_round_trip_all_six_kinds() {
    let groups = [
        window_group("g-window", 0),
        UsageMetricGroupV2 {
            group_id: "g-balance".into(),
            rank: 1,
            kind: UsageMetricGroupKindV2::Balance,
            label: "Credits".into(),
            quota_state: UsageQuotaStateV2::Available,
            value: UsageMetricValueV2::Balance {
                amount: Money::new(12_50, "USD", 2),
                expires_at_epoch: Some(1_900_000_000),
            },
            reset_at_epoch: None,
            ..window_group("g-balance", 1)
        },
        UsageMetricGroupV2 {
            group_id: "g-spend".into(),
            rank: 2,
            kind: UsageMetricGroupKindV2::SpendCap,
            label: "Extra usage".into(),
            quota_state: UsageQuotaStateV2::Warning,
            value: UsageMetricValueV2::SpendCap {
                cap: Some(Money::new(30_000, "USD", 2)),
                spent: Some(Money::new(27_00, "USD", 2)),
                remaining: Some(Money::new(3_00, "USD", 2)),
            },
            ..window_group("g-spend", 2)
        },
        UsageMetricGroupV2 {
            group_id: "g-tokens".into(),
            rank: 3,
            kind: UsageMetricGroupKindV2::TokenTotals,
            label: "Token totals".into(),
            quota_state: UsageQuotaStateV2::NotApplicable,
            value: UsageMetricValueV2::TokenTotals {
                input: Some(1_000),
                output: Some(2_000),
                cached: None,
                reasoning: Some(50),
                interval_label: Some("current period".into()),
            },
            reset_at_epoch: None,
            ..window_group("g-tokens", 3)
        },
        UsageMetricGroupV2 {
            group_id: "g-rate".into(),
            rank: 4,
            kind: UsageMetricGroupKindV2::RateLimit,
            label: "Requests per minute".into(),
            quota_state: UsageQuotaStateV2::Available,
            value: UsageMetricValueV2::RateLimit {
                limit: Some(60),
                remaining: Some(59),
                window_label: Some("per minute".into()),
            },
            ..window_group("g-rate", 4)
        },
        UsageMetricGroupV2 {
            group_id: "g-plan".into(),
            rank: 5,
            kind: UsageMetricGroupKindV2::Plan,
            label: "Plan".into(),
            quota_state: UsageQuotaStateV2::NotApplicable,
            value: UsageMetricValueV2::Plan {
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
            serde_json::from_slice::<UsageMetricGroupV2>(&bytes).unwrap(),
            group.clone()
        );
    }
}

#[test]
fn metric_group_validate_rejects_mismatched_kind_value_and_money() {
    let mut group = window_group("g", 0);
    group.kind = UsageMetricGroupKindV2::Balance;
    group.validate(0).unwrap_err();
    group.kind = UsageMetricGroupKindV2::Window;

    group.value = UsageMetricValueV2::SpendCap {
        cap: Some(Money::new(1_00, "USD", 2)),
        spent: Some(Money::new(50, "SGD", 2)),
        remaining: None,
    };
    group.kind = UsageMetricGroupKindV2::SpendCap;
    group.validate(0).unwrap_err();

    group.value = UsageMetricValueV2::SpendCap {
        cap: Some(Money::new(1_00, "USD", 2)),
        spent: Some(Money::new(50, "USD", 3)),
        remaining: None,
    };
    group.validate(0).unwrap();
}

#[test]
fn metric_group_validate_keeps_reset_renewal_and_issue_scope_separate() {
    let mut group = window_group("g", 0);
    group.renews_at_epoch = Some(1_900_000_000);
    group.validate(0).unwrap_err();
    group.renews_at_epoch = None;

    group.kind = UsageMetricGroupKindV2::Plan;
    group.value = UsageMetricValueV2::Plan {
        plan_label: Some("Pro".into()),
        tier: None,
    };
    group.validate(0).unwrap_err();
    group.reset_at_epoch = None;
    group.renews_at_epoch = Some(1_900_000_000);
    group.validate(0).unwrap();

    group.issues.push(UsageIssueV2 {
        code: "x".into(),
        scope: UsageIssueScopeV2::Account,
        recoverability: UsageIssueRecoverabilityV2::Retryable,
        message: "x".into(),
        retry_at_epoch: None,
    });
    group.validate(0).unwrap_err();
    group.issues[0].scope = UsageIssueScopeV2::Group;
    group.validate(0).unwrap();

    group.rank = 3;
    group.validate(0).unwrap_err();
}

#[test]
fn account_rejects_duplicate_group_ids_and_bad_group_rank() {
    let mut projection: UsageProjectionV2 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v2-current.json"
    ))
    .unwrap();
    projection.providers[0].accounts[0]
        .metric_groups
        .push(window_group("g", 0));
    projection.providers[0].accounts[0]
        .metric_groups
        .push(window_group("g", 1));
    projection.validate().unwrap_err();
    projection.providers[0].accounts[0].metric_groups[1].group_id = "h".into();
    projection.providers[0].accounts[0].metric_groups[1].rank = 7;
    projection.validate().unwrap_err();
    projection.providers[0].accounts[0].metric_groups[1].rank = 1;
    projection.validate().unwrap();
}

#[test]
fn account_without_metric_groups_stays_wire_compatible() {
    let fixture = include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v2-current.json"
    );
    let projection: UsageProjectionV2 = serde_json::from_str(fixture).unwrap();
    assert!(projection.providers[0].accounts[0].metric_groups.is_empty());
    assert_eq!(
        projection.providers[0].accounts[0].credential_expires_at_epoch,
        None
    );
    let encoded = serde_json::to_value(&projection).unwrap();
    assert_eq!(
        encoded,
        serde_json::from_str::<serde_json::Value>(fixture).unwrap()
    );
}

#[test]
fn quota_states_serialize_distinctly() {
    let states = [
        (UsageQuotaStateV2::Available, "available"),
        (UsageQuotaStateV2::NotStarted, "not_started"),
        (UsageQuotaStateV2::Warning, "warning"),
        (UsageQuotaStateV2::Exhausted, "exhausted"),
        (UsageQuotaStateV2::Unsupported, "unsupported"),
        (UsageQuotaStateV2::Unavailable, "unavailable"),
        (UsageQuotaStateV2::NoPermission, "no_permission"),
        (UsageQuotaStateV2::Unknown, "unknown"),
        (UsageQuotaStateV2::NotApplicable, "not_applicable"),
        (UsageQuotaStateV2::Error, "error"),
    ];
    let mut seen = BTreeSet::new();
    for (state, wire) in states {
        let encoded = serde_json::to_value(state).unwrap();
        assert_eq!(encoded, serde_json::Value::String(wire.into()));
        assert!(seen.insert(wire));
        assert_eq!(
            serde_json::from_value::<UsageQuotaStateV2>(encoded).unwrap(),
            state
        );
    }
}

#[test]
fn reset_credential_expiry_and_renewal_are_independent_fields() {
    let mut projection: UsageProjectionV2 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v2-current.json"
    ))
    .unwrap();
    let account = &mut projection.providers[0].accounts[0];
    account.windows[0].reset_at_epoch = Some(1_800_100_000);
    account.credential_expires_at_epoch = Some(1_800_200_000);
    let mut plan = window_group("g-plan", 0);
    plan.kind = UsageMetricGroupKindV2::Plan;
    plan.value = UsageMetricValueV2::Plan {
        plan_label: Some("Pro".into()),
        tier: None,
    };
    plan.quota_state = UsageQuotaStateV2::NotApplicable;
    plan.reset_at_epoch = None;
    plan.renews_at_epoch = Some(1_800_300_000);
    account.metric_groups.push(plan);
    projection.validate().unwrap();
    let round_tripped: UsageProjectionV2 =
        serde_json::from_slice(&serde_json::to_vec(&projection).unwrap()).unwrap();
    let account = &round_tripped.providers[0].accounts[0];
    assert_eq!(account.windows[0].reset_at_epoch, Some(1_800_100_000));
    assert_eq!(account.credential_expires_at_epoch, Some(1_800_200_000));
    assert_eq!(
        account.metric_groups[0].renews_at_epoch,
        Some(1_800_300_000)
    );
    assert_eq!(account.metric_groups[0].reset_at_epoch, None);
}

#[test]
fn quota_scope_dedup_key_is_stable_and_axis_sensitive() {
    let base = UsageQuotaScopeKey::new("kimi-code", "org-1", "subscription");
    assert_eq!(base.dedup_key(), base.dedup_key());
    assert!(base.shares_allowance(&UsageQuotaScopeKey::new(
        "kimi-code",
        "org-1",
        "subscription"
    )));
    for other in [
        UsageQuotaScopeKey::new("moonshot-payg", "org-1", "subscription"),
        UsageQuotaScopeKey::new("kimi-code", "org-2", "subscription"),
        UsageQuotaScopeKey::new("kimi-code", "org-1", "key"),
        UsageQuotaScopeKey::new("kimi-code", "org-1", "subscription").with_model("kimi-k2"),
        UsageQuotaScopeKey::new("kimi-code", "org-1", "subscription").with_key("key-a"),
    ] {
        assert!(!base.shares_allowance(&other));
        assert_ne!(base.dedup_key(), other.dedup_key());
    }
    // Length-prefixing keeps ("ab","c") distinct from ("a","bc").
    let left = UsageQuotaScopeKey::new("ab", "c", "s");
    let right = UsageQuotaScopeKey::new("a", "bc", "s");
    assert_ne!(left.dedup_key(), right.dedup_key());
    // Missing scope detail never equals an empty string.
    let missing = UsageQuotaScopeKey::new("s", "b", "c");
    let empty_key = UsageQuotaScopeKey::new("s", "b", "c").with_key("");
    assert!(!missing.shares_allowance(&empty_key));
    assert_ne!(missing.dedup_key(), empty_key.dedup_key());
}

#[test]
fn independent_key_caps_never_merge() {
    let key_a = UsageQuotaScopeKey::new("openai", "org-1", "key").with_key("key-a");
    let key_b = UsageQuotaScopeKey::new("openai", "org-1", "key").with_key("key-b");
    assert!(!key_a.shares_allowance(&key_b));
    assert_ne!(key_a.dedup_key(), key_b.dedup_key());
    // Same key identity under one billing subject shares one observation.
    let key_a_alias = UsageQuotaScopeKey::new("openai", "org-1", "key").with_key("key-a");
    assert!(key_a.shares_allowance(&key_a_alias));
    assert_eq!(key_a.dedup_key(), key_a_alias.dedup_key());
    // Unscoped subscription allowance never merges with a key cap.
    let subscription = UsageQuotaScopeKey::new("openai", "org-1", "subscription");
    assert!(!subscription.shares_allowance(&key_a));
}

#[test]
fn canonical_projection_v2_forty_account_fixture_stays_below_transport_margin() {
    let mut projection: UsageProjectionV2 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v2-current.json"
    ))
    .unwrap();
    let seed = projection.providers[0].accounts[0].clone();
    projection.providers[0].accounts = (0..40)
        .map(|rank| {
            let mut account = seed.clone();
            account.rank = rank;
            account.canonical_account_id = format!("account-{rank:02}");
            account.refresh_capabilities = vec![UsageAccountCapability {
                account_id: format!("route-{rank:02}"),
                surface_id: "codex".into(),
            }];
            account.display_label = format!("account-{rank:02}@example.test");
            account
        })
        .collect();
    projection.validate().unwrap();
    let encoded = serde_json::to_vec(&projection).unwrap();
    assert!(
        encoded.len() < USAGE_BROKER_MAX_FRAME_BYTES * 3 / 4,
        "40-account fixture is {} bytes",
        encoded.len()
    );
}

fn request_count_quota(remaining: Option<u64>, limit: Option<u64>) -> crate::control::CountQuota {
    crate::control::CountQuota {
        used: Some(u64::MAX),
        limit,
        remaining,
        unit: crate::control::CountQuotaUnit::Requests,
        period: crate::control::CountQuotaPeriod::UtcDaily,
        provenance: crate::control::CountQuotaProvenance::ProviderReported,
    }
}

#[test]
fn count_quota_roundtrip_preserves_full_unsigned_domain_and_unknowns() {
    for remaining in [None, Some(0), Some(1), Some(u64::MAX)] {
        let quota = request_count_quota(remaining, Some(u64::MAX));
        let json = serde_json::to_string(&quota).unwrap();
        let decoded: crate::control::CountQuota = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, quota);
        assert!(json.contains("18446744073709551615"));
    }
    assert!(
        serde_json::from_value::<crate::control::CountQuota>(serde_json::json!({
            "used": -1, "limit": 1, "remaining": 0, "unit": "requests",
            "period": "utc_daily", "provenance": "provider_reported"
        }))
        .is_err()
    );
}

#[test]
fn count_geometry_cannot_establish_exhaustion() {
    let one = request_count_quota(Some(1), Some(1000));
    assert_eq!(one.remaining_percent(), Some(0));
    assert!(validate_count_quota("test", Some(&one), UsageQuotaStateV2::Available).is_ok());
    assert!(validate_count_quota("test", Some(&one), UsageQuotaStateV2::Exhausted).is_err());
    let zero = request_count_quota(Some(0), Some(1000));
    assert!(validate_count_quota("test", Some(&zero), UsageQuotaStateV2::Exhausted).is_ok());
    assert!(validate_count_quota("test", Some(&zero), UsageQuotaStateV2::Available).is_err());
    let unknown = request_count_quota(None, Some(1000));
    assert!(validate_count_quota("test", Some(&unknown), UsageQuotaStateV2::Unknown).is_ok());
    assert!(validate_count_quota("test", Some(&unknown), UsageQuotaStateV2::Exhausted).is_err());
    assert_eq!(
        request_count_quota(Some(0), Some(0)).remaining_percent(),
        None
    );
    assert_eq!(
        request_count_quota(Some(u64::MAX), Some(u64::MAX)).remaining_percent(),
        Some(100)
    );
}

#[test]
fn canonical_count_group_rejects_rounded_exhaustion_and_preserves_independent_counts() {
    let mut group = count_window_group();
    if let UsageMetricValueV2::Window {
        count_quota,
        remaining_percent,
        remaining_raw_percent,
        ..
    } = &mut group.value
    {
        *count_quota = Some(request_count_quota(Some(1), Some(1000)));
        *remaining_percent = Some(UsagePercent::new(0).unwrap());
        *remaining_raw_percent = Some(0);
    }
    assert!(group.validate(0).is_ok());
    group.quota_state = UsageQuotaStateV2::Exhausted;
    assert!(group.validate(0).is_err());
    let json = serde_json::to_string(&group).unwrap();
    assert_eq!(
        serde_json::from_str::<UsageMetricGroupV2>(&json).unwrap(),
        group
    );
}

#[test]
fn canonical_principal_count_window_requires_exact_state() {
    let mut window = UsageLimitWindowV2 {
        window_id: "requests".into(),
        rank: 0,
        category: UsageWindowCategoryV2::LongRange,
        label: "Daily requests".into(),
        value_label: "1 of 1000 requests remaining".into(),
        reset_label: "midnight UTC".into(),
        remaining_percent: Some(UsagePercent::new(0).unwrap()),
        remaining_raw_percent: Some(0),
        used_percent: None,
        used_raw_percent: None,
        reset_at_epoch: None,
        quota_state: UsageQuotaStateV2::Available,
        count_quota: Some(request_count_quota(Some(1), Some(1000))),
        pace_label: None,
        runs_out_label: None,
    };
    assert!(window.validate(0).is_ok());
    assert_eq!(
        serde_json::from_str::<UsageLimitWindowV2>(&serde_json::to_string(&window).unwrap())
            .unwrap(),
        window
    );
    window.quota_state = UsageQuotaStateV2::Exhausted;
    assert!(window.validate(0).is_err());
    window.count_quota.as_mut().unwrap().remaining = Some(0);
    assert!(window.validate(0).is_ok());
    window.count_quota.as_mut().unwrap().remaining = None;
    assert!(window.validate(0).is_err());
    window.quota_state = UsageQuotaStateV2::Unknown;
    window.remaining_percent = None;
    window.remaining_raw_percent = None;
    assert!(window.validate(0).is_ok());
}

#[test]
fn canonical_count_geometry_rejects_unknown_zero_cap_and_mismatched_percent() {
    let mut group = count_window_group();
    if let UsageMetricValueV2::Window { count_quota, .. } = &mut group.value {
        *count_quota = Some(request_count_quota(Some(1), Some(1000)));
    }
    assert!(group.validate(0).is_err());
    if let UsageMetricValueV2::Window {
        remaining_percent,
        remaining_raw_percent,
        ..
    } = &mut group.value
    {
        *remaining_percent = Some(UsagePercent::new(0).unwrap());
        *remaining_raw_percent = Some(0);
    }
    assert!(group.validate(0).is_ok());
    if let UsageMetricValueV2::Window { count_quota, .. } = &mut group.value {
        count_quota.as_mut().unwrap().limit = Some(0);
    }
    assert!(group.validate(0).is_err());
    if let UsageMetricValueV2::Window {
        remaining_percent,
        remaining_raw_percent,
        ..
    } = &mut group.value
    {
        *remaining_percent = None;
        *remaining_raw_percent = None;
    }
    assert!(group.validate(0).is_ok());
}

fn count_window_group() -> UsageMetricGroupV2 {
    let mut group = window_group("count-window", 0);
    if let UsageMetricValueV2::Window { period, unit, .. } = &mut group.value {
        *period = UsageMetricPeriodV2::Calendar {
            granularity: UsageCalendarPeriodV2::Daily,
        };
        *unit = Some("requests".into());
    }
    group
}

#[test]
fn canonical_count_window_rejects_conflicting_unit_and_period() {
    let mut group = count_window_group();
    if let UsageMetricValueV2::Window { count_quota, .. } = &mut group.value {
        *count_quota = Some(request_count_quota(Some(57), Some(100)));
    }
    assert!(group.validate(0).is_ok());
    if let UsageMetricValueV2::Window { unit, .. } = &mut group.value {
        *unit = Some("USD".into());
    }
    assert!(group.validate(0).is_err());
    if let UsageMetricValueV2::Window { unit, period, .. } = &mut group.value {
        *unit = Some("requests".into());
        *period = UsageMetricPeriodV2::Unknown;
    }
    assert!(group.validate(0).is_err());
    if let UsageMetricValueV2::Window { count_quota, .. } = &mut group.value {
        count_quota.as_mut().unwrap().period = crate::control::CountQuotaPeriod::Unknown;
    }
    assert!(group.validate(0).is_ok());
}

#[test]
fn count_states_reject_semantic_mismatches_and_preserve_degradation() {
    for remaining in [Some(0), Some(1)] {
        let quota = request_count_quota(remaining, Some(1000));
        for state in [
            UsageQuotaStateV2::Unknown,
            UsageQuotaStateV2::NotApplicable,
            UsageQuotaStateV2::NotStarted,
        ] {
            assert!(validate_count_quota("test", Some(&quota), state).is_err());
        }
        for state in [
            UsageQuotaStateV2::Error,
            UsageQuotaStateV2::Unavailable,
            UsageQuotaStateV2::NoPermission,
            UsageQuotaStateV2::Unsupported,
        ] {
            assert!(validate_count_quota("test", Some(&quota), state).is_ok());
        }
    }
    let unknown = request_count_quota(None, Some(1000));
    assert!(validate_count_quota("test", Some(&unknown), UsageQuotaStateV2::NotStarted).is_err());
    assert!(validate_count_quota("test", Some(&unknown), UsageQuotaStateV2::Warning).is_err());
}

#[test]
fn projection_rejects_unresolved_grants_without_exact_diagnostic_references() {
    let mut projection: UsageProjectionV2 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v2-current.json"
    ))
    .unwrap();
    let grant = UsageUnresolvedGrantV2 {
        configured_account_id: "configured-personal".into(),
        surface_id: "codex".into(),
        issues: vec![UsageIssueV2 {
            code: "credential_missing".into(),
            scope: UsageIssueScopeV2::Account,
            recoverability: UsageIssueRecoverabilityV2::ActionRequired,
            message: "Credentials are missing".into(),
            retry_at_epoch: None,
        }],
    };
    projection.unresolved_grants = vec![grant.clone()];
    projection.validate().unwrap();
    for field in [
        "account",
        "surface",
        "issues",
        "code",
        "message",
        "scope",
        "control",
        "unknown_surface",
    ] {
        let mut altered = grant.clone();
        match field {
            "account" => altered.configured_account_id.clear(),
            "surface" => altered.surface_id.clear(),
            "unknown_surface" => altered.surface_id = "unknown".into(),
            "issues" => altered.issues.clear(),
            "code" => altered.issues[0].code.clear(),
            "message" => altered.issues[0].message.clear(),
            "scope" => altered.issues[0].scope = UsageIssueScopeV2::Projection,
            _ => altered.configured_account_id.push('\n'),
        }
        projection.unresolved_grants = vec![altered];
        assert!(projection.validate().is_err(), "{field}");
    }
    projection.unresolved_grants = vec![grant.clone(), grant];
    assert!(projection.validate().is_err());
}

#[test]
fn spend_cap_allows_exact_mixed_scales_and_negative_remaining() {
    let mut group = window_group("tiny-spend", 0);
    group.kind = UsageMetricGroupKindV2::SpendCap;
    group.value = UsageMetricValueV2::SpendCap {
        cap: Some(Money::new(1, "USD", 3)),
        spent: Some(Money::new(1, "USD", 4)),
        remaining: Some(Money::new(9, "USD", 4)),
    };
    assert!(group.validate(0).is_ok());
    let json = serde_json::to_string(&group).unwrap();
    assert_eq!(
        serde_json::from_str::<UsageMetricGroupV2>(&json).unwrap(),
        group
    );
    if let UsageMetricValueV2::SpendCap { remaining, .. } = &mut group.value {
        *remaining = Some(Money::new(-1, "USD", 255));
    }
    assert!(group.validate(0).is_ok());
}

#[test]
fn spend_cap_rejects_negative_cap_spend_and_currency_mismatch() {
    let mut group = window_group("spend", 0);
    group.kind = UsageMetricGroupKindV2::SpendCap;
    for (cap, spent, remaining) in [
        (
            Money::new(-1, "USD", 3),
            Money::new(0, "USD", 4),
            Money::new(0, "USD", 3),
        ),
        (
            Money::new(1, "USD", 3),
            Money::new(-1, "USD", 4),
            Money::new(0, "USD", 3),
        ),
        (
            Money::new(1, "USD", 3),
            Money::new(1, "SGD", 4),
            Money::new(0, "USD", 3),
        ),
    ] {
        group.value = UsageMetricValueV2::SpendCap {
            cap: Some(cap),
            spent: Some(spent),
            remaining: Some(remaining),
        };
        assert!(group.validate(0).is_err());
    }
}

#[test]
fn tunnel_cancellation_roundtrips_and_rejects_unframed_legacy_request() {
    let cancel = UsageRelayTunnelMessage::Cancel { request_id: 7 };
    let bytes = serde_json::to_vec(&cancel).unwrap();
    assert_eq!(
        serde_json::from_slice::<UsageRelayTunnelMessage>(&bytes).unwrap(),
        cancel
    );
    assert!(
        serde_json::from_str::<UsageRelayTunnelMessage>(r#"{"request_id":7,"request":{}}"#)
            .is_err()
    );
}
