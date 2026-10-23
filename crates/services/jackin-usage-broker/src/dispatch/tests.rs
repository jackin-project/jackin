// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn v7_and_v8_request_envelopes_are_rejected_by_the_v9_dispatch_gate() {
    for version in ["v7", "v8"] {
        let request: UsageBrokerRequest = serde_json::from_value(serde_json::json!({
            "protocol_version": version,
            "build_id": "test-build",
            "operation": {"operation": "current_projection"}
        }))
        .expect("older request envelope fixture should decode");
        assert_eq!(request.operation, UsageBrokerOperation::CurrentProjection);
        assert!(!request_header_matches_current_protocol(
            &request.protocol_version,
            &request.build_id,
            "test-build"
        ));
    }

    assert!(request_header_matches_current_protocol(
        USAGE_BROKER_PROTOCOL_VERSION,
        "test-build",
        "test-build"
    ));
    assert!(!request_header_matches_current_protocol(
        USAGE_BROKER_PROTOCOL_VERSION,
        "older-build",
        "test-build"
    ));
}

#[test]
fn future_request_envelope_is_rejected_by_the_current_dispatch_gate() {
    let future_version = USAGE_BROKER_PROTOCOL_VERSION
        .strip_prefix('v')
        .and_then(|version| version.parse::<u16>().ok())
        .and_then(|version| version.checked_add(1))
        .map(|version| format!("v{version}"))
        .expect("current protocol version has a numeric successor");
    let request: UsageBrokerRequest = serde_json::from_value(serde_json::json!({
        "protocol_version": future_version,
        "build_id": "test-build",
        "operation": {"operation": "current_projection"}
    }))
    .expect("future request envelope fixture should decode");

    assert_eq!(request.operation, UsageBrokerOperation::CurrentProjection);
    assert!(!request_header_matches_current_protocol(
        &request.protocol_version,
        &request.build_id,
        "test-build"
    ));
}
