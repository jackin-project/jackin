use super::*;
use crate::control::Money;

fn capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "opaque-account".into(),
        surface_id: "claude".into(),
    }
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
                capability: capability(),
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
    let projection: UsageProjectionV1 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
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
fn canonical_projection_v1_round_trips_frozen_fixture() {
    let fixture = include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
    );
    let projection: UsageProjectionV1 = serde_json::from_str(fixture).unwrap();
    projection.validate().unwrap();
    let encoded = serde_json::to_value(&projection).unwrap();
    let original: serde_json::Value = serde_json::from_str(fixture).unwrap();
    assert_eq!(encoded, original);
}

#[test]
fn canonical_projection_v1_rejects_unknown_major() {
    let error = serde_json::from_str::<UsageProjectionV1>(
        r#"{"schema_version":2,"projection_id":"p","generated_at_epoch":0,"discovery_revision":"d","broker_instance_id":"b","broker_generation":0,"refresh_state":"idle","providers":[],"unresolved":[],"issues":[]}"#,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsupported usage projection schema")
    );
}

#[test]
fn canonical_projection_v1_rejects_invalid_percent_and_cross_field_shape() {
    serde_json::from_str::<UsagePercent>("101").unwrap_err();
    let mut projection: UsageProjectionV1 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
    ))
    .unwrap();
    let window = &mut projection.providers[0].accounts[0].windows[0];
    window.used_percent = window.remaining_percent;
    projection.validate().unwrap_err();
}

fn window_group(group_id: &str, rank: u32) -> UsageMetricGroupV1 {
    UsageMetricGroupV1 {
        group_id: group_id.into(),
        rank,
        kind: UsageMetricGroupKindV1::Window,
        label: "Weekly".into(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: Some(1_800_000_000),
        fetched_at_epoch: 1_800_000_001,
        last_success_at_epoch: Some(1_800_000_000),
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::Window {
            remaining_percent: Some(UsagePercent::new(57).unwrap()),
            remaining_raw_percent: Some(57),
            used_percent: None,
            used_raw_percent: None,
            period: UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Weekly,
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
    let mut projection: UsageProjectionV1 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
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

#[test]
fn account_rejects_duplicate_group_ids_and_bad_group_rank() {
    let mut projection: UsageProjectionV1 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
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
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
    );
    let projection: UsageProjectionV1 = serde_json::from_str(fixture).unwrap();
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
        (UsageQuotaStateV1::Available, "available"),
        (UsageQuotaStateV1::NotStarted, "not_started"),
        (UsageQuotaStateV1::Warning, "warning"),
        (UsageQuotaStateV1::Exhausted, "exhausted"),
        (UsageQuotaStateV1::Unsupported, "unsupported"),
        (UsageQuotaStateV1::Unavailable, "unavailable"),
        (UsageQuotaStateV1::NoPermission, "no_permission"),
        (UsageQuotaStateV1::Unknown, "unknown"),
        (UsageQuotaStateV1::NotApplicable, "not_applicable"),
        (UsageQuotaStateV1::Error, "error"),
    ];
    let mut seen = BTreeSet::new();
    for (state, wire) in states {
        let encoded = serde_json::to_value(state).unwrap();
        assert_eq!(encoded, serde_json::Value::String(wire.into()));
        assert!(seen.insert(wire));
        assert_eq!(
            serde_json::from_value::<UsageQuotaStateV1>(encoded).unwrap(),
            state
        );
    }
}

#[test]
fn reset_credential_expiry_and_renewal_are_independent_fields() {
    let mut projection: UsageProjectionV1 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
    ))
    .unwrap();
    let account = &mut projection.providers[0].accounts[0];
    account.windows[0].reset_at_epoch = Some(1_800_100_000);
    account.credential_expires_at_epoch = Some(1_800_200_000);
    let mut plan = window_group("g-plan", 0);
    plan.kind = UsageMetricGroupKindV1::Plan;
    plan.value = UsageMetricValueV1::Plan {
        plan_label: Some("Pro".into()),
        tier: None,
    };
    plan.quota_state = UsageQuotaStateV1::NotApplicable;
    plan.reset_at_epoch = None;
    plan.renews_at_epoch = Some(1_800_300_000);
    account.metric_groups.push(plan);
    projection.validate().unwrap();
    let round_tripped: UsageProjectionV1 =
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
fn canonical_projection_v1_forty_account_fixture_stays_below_transport_margin() {
    let mut projection: UsageProjectionV1 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
    ))
    .unwrap();
    let seed = projection.providers[0].accounts[0].clone();
    projection.providers[0].accounts = (0..40)
        .map(|rank| {
            let mut account = seed.clone();
            account.rank = rank;
            account.canonical_account_id = format!("account-{rank:02}");
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
