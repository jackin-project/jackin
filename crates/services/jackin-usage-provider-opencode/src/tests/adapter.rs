// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::UsageSnapshotStatus;

use crate::{
    OpenCodeUsageError, classify_opencode_http_error, load_opencode_api_key, parse_opencode_usage,
};

#[test]
fn auth_error_body_maps_to_key_error() {
    assert_eq!(
        classify_opencode_http_error(500, r#"{"error":{"type":"AuthError"}}"#),
        OpenCodeUsageError::Key("OpenCode usage HTTP 500 (invalid API key)".to_owned())
    );
}

#[test]
fn entitlement_error_body_maps_to_entitlement_error() {
    assert_eq!(
        classify_opencode_http_error(500, r#"{"error":{"type":"EntitlementError"}}"#),
        OpenCodeUsageError::Entitlement(
            "OpenCode usage HTTP 500 (Go subscription required)".to_owned()
        )
    );
}

#[test]
fn bare_status_codes_map_without_body() {
    assert_eq!(
        classify_opencode_http_error(401, ""),
        OpenCodeUsageError::Key("OpenCode usage HTTP 401 (invalid API key)".to_owned())
    );
    assert_eq!(
        classify_opencode_http_error(403, ""),
        OpenCodeUsageError::Entitlement(
            "OpenCode usage HTTP 403 (Go subscription required)".to_owned()
        )
    );
    assert_eq!(
        classify_opencode_http_error(500, ""),
        OpenCodeUsageError::Http("OpenCode usage HTTP 500".to_owned())
    );
}

#[test]
fn usage_without_usage_object_is_malformed() {
    let error = parse_opencode_usage(serde_json::json!({}), 1_700_000_000)
        .expect_err("empty body must fail");

    assert_eq!(error, "OpenCode usage response is malformed");
}

#[test]
fn missing_windows_yield_unavailable_buckets_without_rate_limit() {
    let quota = parse_opencode_usage(serde_json::json!({"usage": {}}), 1_700_000_000)
        .expect("empty usage object must parse");

    assert!(!quota.rate_limited);
    assert_eq!(quota.buckets.len(), 3);
    assert!(
        quota
            .buckets
            .iter()
            .all(|bucket| bucket.status == UsageSnapshotStatus::Unavailable)
    );
}

#[test]
fn auth_file_loading_accepts_single_trimmed_key() {
    let dir = tempfile::tempdir().expect("test tempdir must exist");
    let path = dir.path().join("auth.json");
    std::fs::write(&path, r#"{"opencode-go":{"type":"api","key":"  k123  "}}"#)
        .expect("auth fixture must be writable");

    assert_eq!(load_opencode_api_key(&path).as_deref(), Ok("k123"));
}

#[test]
fn auth_file_loading_rejects_problem_shapes() {
    let dir = tempfile::tempdir().expect("test tempdir must exist");

    let missing = dir.path().join("absent.json");
    assert!(
        load_opencode_api_key(&missing)
            .expect_err("missing auth file must fail")
            .contains("missing")
    );

    let multiple = dir.path().join("multiple.json");
    std::fs::write(
        &multiple,
        r#"{"opencode-go":{"type":"api","key":"k1"},"other":{"type":"api","key":"k2"}}"#,
    )
    .expect("auth fixture must be writable");
    assert!(
        load_opencode_api_key(&multiple)
            .expect_err("multiple credentials must fail")
            .contains("multiple")
    );
}
