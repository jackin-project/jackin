// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn v7_request_envelope_is_rejected_by_the_v8_dispatch_gate() {
    let request: UsageBrokerRequest = serde_json::from_str(
            r#"{"protocol_version":"v7","build_id":"test-build","operation":{"operation":"current_projection"}}"#,
        )
        .expect("v7 request envelope fixture should decode");
    assert_eq!(request.operation, UsageBrokerOperation::CurrentProjection);

    assert!(!request_header_matches_current_protocol(
        &request.protocol_version,
        &request.build_id,
        "test-build"
    ));
    assert!(!request_header_matches_current_protocol(
        "v9",
        "test-build",
        "test-build"
    ));
    assert!(request_header_matches_current_protocol(
        USAGE_BROKER_PROTOCOL_VERSION,
        "test-build",
        "test-build"
    ));
}
