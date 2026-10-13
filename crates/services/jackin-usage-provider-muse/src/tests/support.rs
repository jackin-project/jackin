// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) const MUSE_USAGE_READ_FIXTURE: &str = r#"{
    "usage": {
        "observedAtMs": 1780000000000,
        "tier": "Muse Pro",
        "window": {
            "usedPercent": 42.5,
            "resetsAtMs": 1780018000000,
            "windowDurationMins": 300
        },
        "weekly": {"usedPercent": 73, "resetsAtMs": 1780600000000}
    }
}"#;

pub(super) const MUSE_AUTH_JSON_FIXTURE: &str = r#"{
    "schema_version": 2,
    "providers": {
        "meta": {
            "mechanism": "oauth",
            "storage": "keychain",
            "obtained_via": "login",
            "api_base_url": "https://api.meta.ai",
            "user_full_name": "Example Operator",
            "user_email": "operator@example.com"
        }
    }
}"#;

pub(super) fn fixture_value(fixture: &str) -> serde_json::Value {
    serde_json::from_str(fixture).expect("fixture parses")
}

pub(super) fn assert_near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < f64::EPSILON,
        "expected {expected}, got {actual}"
    );
}
