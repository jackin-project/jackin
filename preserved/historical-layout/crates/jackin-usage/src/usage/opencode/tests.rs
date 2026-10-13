// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn window_fixture(status: &str, body: serde_json::Value) -> serde_json::Value {
    let mut window = serde_json::Map::new();
    window.insert(
        "status".to_owned(),
        serde_json::Value::String(status.to_owned()),
    );
    if let serde_json::Value::Object(extra) = body {
        window.extend(extra);
    }
    serde_json::Value::Object(window)
}

fn usage_fixture(
    rolling: serde_json::Value,
    weekly: serde_json::Value,
    monthly: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "usage": {"rolling": rolling, "weekly": weekly, "monthly": monthly}
    })
}

#[test]
fn opencode_percent_spellings_and_used_limit_fallback() {
    let quota = parse_opencode_usage(
        usage_fixture(
            window_fixture(
                "ok",
                serde_json::json!({"percent": 20, "resetsAt": "2026-08-21T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"usagePercent": 30, "reset_at": "2026-08-25T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"used": 5, "limit": 100, "resetInSec": 3600}),
            ),
        ),
        1_776_000_000,
    )
    .expect("tolerant fixture");
    assert_eq!(quota.buckets.len(), 3);
    assert_eq!(quota.buckets[0].remaining_percent, Some(80));
    assert_eq!(quota.buckets[1].remaining_percent, Some(70));
    assert_eq!(quota.buckets[2].remaining_percent, Some(95));
    assert_eq!(quota.buckets[2].resets_at, Some(1_776_003_600));
}

#[test]
fn opencode_one_percent_means_one_percent() {
    let quota = parse_opencode_usage(
        usage_fixture(
            window_fixture(
                "ok",
                serde_json::json!({"percent": 1, "resetsAt": "2026-08-21T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"percent": 0, "resetsAt": "2026-08-25T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"percent": 0, "resetsAt": "2026-09-01T12:00:00Z"}),
            ),
        ),
        1_776_000_000,
    )
    .expect("one-percent fixture");
    assert_eq!(quota.buckets[0].remaining_percent, Some(99));
}

#[test]
fn opencode_per_model_and_zen_fields_render_nothing() {
    let mut payload = usage_fixture(
        window_fixture(
            "ok",
            serde_json::json!({
                "percent": 20,
                "resetsAt": "2026-08-21T12:00:00Z",
                "models": {"acme-model": {"percent": 99}}
            }),
        ),
        window_fixture(
            "ok",
            serde_json::json!({"percent": 10, "resetsAt": "2026-08-25T12:00:00Z"}),
        ),
        window_fixture(
            "ok",
            serde_json::json!({"percent": 5, "resetsAt": "2026-09-01T12:00:00Z"}),
        ),
    );
    payload["zenBalanceUSD"] = serde_json::json!(42.0);
    let quota = parse_opencode_usage(payload, 1_776_000_000).expect("extra fields");
    assert_eq!(quota.buckets.len(), 3);
    assert_eq!(quota.buckets[0].remaining_percent, Some(80));
}

#[test]
fn opencode_entitlement_403_is_typed_apart_from_key_failure() {
    let entitlement = classify_opencode_http_error(
        403,
        r#"{"error": {"type": "EntitlementError", "message": "OpenCode Go subscription required."}}"#,
    );
    assert!(matches!(entitlement, OpenCodeUsageError::Entitlement(_)));
    // In-band type wins over the status code.
    let entitlement_on_odd_status =
        classify_opencode_http_error(200, r#"{"error": {"type": "EntitlementError"}}"#);
    assert!(matches!(
        entitlement_on_odd_status,
        OpenCodeUsageError::Entitlement(_)
    ));
    let auth = classify_opencode_http_error(
        401,
        r#"{"error": {"type": "AuthError", "message": "Invalid key"}}"#,
    );
    assert!(matches!(auth, OpenCodeUsageError::Key(_)));
    let bare_403 = classify_opencode_http_error(403, "forbidden");
    assert!(matches!(bare_403, OpenCodeUsageError::Entitlement(_)));
    let server = classify_opencode_http_error(500, "boom");
    assert!(matches!(server, OpenCodeUsageError::Http(_)));
}

#[test]
fn opencode_missing_window_and_negative_percent_stay_errors() {
    let missing = parse_opencode_usage(serde_json::json!({"usage": {}}), 1_776_000_000);
    assert_eq!(missing.unwrap_err(), "OpenCode Rolling window is missing");
    let negative = parse_opencode_usage(
        usage_fixture(
            window_fixture(
                "ok",
                serde_json::json!({"percent": -5, "resetsAt": "2026-08-21T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"percent": 0, "resetsAt": "2026-08-25T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"percent": 0, "resetsAt": "2026-09-01T12:00:00Z"}),
            ),
        ),
        1_776_000_000,
    );
    assert_eq!(
        negative.unwrap_err(),
        "OpenCode Rolling percentage is invalid"
    );
}

#[test]
fn opencode_over_cap_keeps_raw_percent_without_failing_siblings() {
    let quota = parse_opencode_usage(
        usage_fixture(
            window_fixture(
                "ok",
                serde_json::json!({"percent": 101, "resetsAt": "2026-08-21T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"percent": 30, "resetsAt": "2026-08-25T12:00:00Z"}),
            ),
            window_fixture(
                "ok",
                serde_json::json!({"percent": 0, "resetsAt": "2026-09-01T12:00:00Z"}),
            ),
        ),
        1_776_000_000,
    )
    .expect("over-cap is a valid reading");
    assert_eq!(quota.buckets.len(), 3);
    assert_eq!(quota.buckets[0].used_label.as_deref(), Some("101% used"));
    assert_eq!(quota.buckets[0].remaining_percent, Some(0));
    assert_eq!(quota.buckets[1].remaining_percent, Some(70));
    assert_eq!(quota.buckets[1].used_label, None);
}
