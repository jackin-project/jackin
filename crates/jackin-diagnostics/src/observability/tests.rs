// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for observability setup.

use super::rewrite_endpoint_for_container;

#[test]
fn loopback_is_rewritten_to_host_gateway() {
    let rewritten = rewrite_endpoint_for_container("http://127.0.0.1:4318");
    assert_eq!(rewritten.endpoint, "http://host.docker.internal:4318");
    assert!(rewritten.needs_host_gateway);

    let with_path = rewrite_endpoint_for_container("http://localhost:4318/v1/traces");
    assert_eq!(
        with_path.endpoint,
        "http://host.docker.internal:4318/v1/traces"
    );
    assert!(with_path.needs_host_gateway);
}

#[test]
fn routable_host_is_left_alone() {
    let rewritten = rewrite_endpoint_for_container("http://otel.internal:4318");
    assert_eq!(rewritten.endpoint, "http://otel.internal:4318");
    assert!(!rewritten.needs_host_gateway);
}
