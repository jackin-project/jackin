use super::*;

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
fn scoped_surface_request_round_trip_exposes_no_capability() {
    let request = UsageBrokerRequest {
        protocol_version: USAGE_BROKER_PROTOCOL_VERSION.into(),
        build_id: "test-build".into(),
        operation: UsageBrokerOperation::RefreshForSurface {
            surface_id: "claude".into(),
            observed_generation: 3,
            force: false,
        },
    };

    let bytes = serde_json::to_vec(&request).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("account_id"));
    assert_eq!(
        serde_json::from_slice::<UsageBrokerRequest>(&bytes).unwrap(),
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
            operation: UsageBrokerOperation::CurrentForSurface {
                surface_id: "claude".to_owned(),
            },
        },
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("account_id"));
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
