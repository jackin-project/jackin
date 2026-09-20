// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    MuseKeyExchangePolicy, muse_buckets, muse_freshness_epoch, muse_identity_from_value, muse_view,
    parse_muse_usage_read,
};
use super::{UsageConfidence, UsageSnapshotStatus, UsageSource};

/// Sanitized MSP `usage/read` fixture: observation shape only, no secrets.
const MUSE_USAGE_READ_FIXTURE: &str = r#"{
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

const MUSE_AUTH_JSON_FIXTURE: &str = r#"{
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

fn fixture_value(fixture: &str) -> serde_json::Value {
    serde_json::from_str(fixture).expect("fixture parses")
}

fn assert_near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < f64::EPSILON,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn parses_full_observation() {
    let observation = parse_muse_usage_read(fixture_value(MUSE_USAGE_READ_FIXTURE))
        .expect("fixture parses")
        .expect("usage present");
    assert_eq!(observation.observed_at_ms, 1_780_000_000_000);
    assert_eq!(observation.tier.as_deref(), Some("Muse Pro"));
    let window = observation.window.expect("window present");
    assert_near(window.used_percent, 42.5);
    assert_eq!(window.resets_at, Some(1_780_018_000));
    assert_eq!(window.window_duration_mins, Some(300));
    let weekly = observation.weekly.expect("weekly present");
    assert_near(weekly.used_percent, 73.0);
    assert_eq!(weekly.resets_at, Some(1_780_600_000));
    assert_eq!(weekly.window_duration_mins, None);
}

#[test]
fn omitted_usage_is_no_observation() {
    for fixture in [r"{}", r#"{"usage": null}"#] {
        let observation = parse_muse_usage_read(fixture_value(fixture)).expect("parses");
        assert!(observation.is_none(), "fixture: {fixture}");
    }
}

#[test]
fn over_cap_percent_preserved_raw() {
    let value = fixture_value(
        r#"{"usage": {"observedAtMs": 1780000000000,
            "window": {"usedPercent": 142.5}}}"#,
    );
    let observation = parse_muse_usage_read(value)
        .expect("parses")
        .expect("usage present");
    let buckets = muse_buckets(&observation, 1_780_000_000);
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].used_label.as_deref(), Some("142.5% used"));
    assert_eq!(buckets[0].remaining_percent, Some(0));
}

#[test]
fn window_label_comes_from_duration() {
    let observation = parse_muse_usage_read(fixture_value(MUSE_USAGE_READ_FIXTURE))
        .expect("parses")
        .expect("usage present");
    let buckets = muse_buckets(&observation, 1_780_000_000);
    assert_eq!(buckets[0].label, "5 hours window");
    assert_eq!(buckets[0].used_label.as_deref(), Some("42.5% used"));
    assert_eq!(buckets[0].remaining_percent, Some(58));
    assert_eq!(buckets[1].label, "Weekly");
    assert_eq!(buckets[1].remaining_percent, Some(27));
}

#[test]
fn reread_with_same_observation_keeps_freshness() {
    assert_eq!(muse_freshness_epoch(1000, 50, 50, 2000), 1000);
}

#[test]
fn reread_with_new_observation_stamps_now() {
    assert_eq!(muse_freshness_epoch(1000, 50, 51, 2000), 2000);
}

#[test]
fn key_exchange_is_never_a_poller() {
    assert!(!MuseKeyExchangePolicy::polling_enabled());
    assert_eq!(
        MuseKeyExchangePolicy::URL,
        "https://api.meta.ai/muse-code/key"
    );
    for secret in MuseKeyExchangePolicy::SECRET_FIELDS {
        assert!(
            !MuseKeyExchangePolicy::ALLOWED_FIELDS.contains(secret),
            "secret field allow-listed: {secret}"
        );
    }
}

#[test]
fn identity_from_auth_json() {
    let identity =
        muse_identity_from_value(&fixture_value(MUSE_AUTH_JSON_FIXTURE)).expect("identity");
    assert_eq!(identity.email.as_deref(), Some("operator@example.com"));
    assert_eq!(identity.full_name.as_deref(), Some("Example Operator"));
}

#[test]
fn identity_absent_is_none() {
    assert!(muse_identity_from_value(&fixture_value(r"{}")).is_none());
    assert!(
        muse_identity_from_value(&fixture_value(
            r#"{"providers": {"meta": {"mechanism": "oauth"}}}"#
        ))
        .is_none()
    );
}

#[test]
fn negative_percent_rejected() {
    let result = parse_muse_usage_read(fixture_value(
        r#"{"usage": {"observedAtMs": 1780000000000,
            "window": {"usedPercent": -1}}}"#,
    ));
    assert_eq!(result, Err("Muse window usedPercent is invalid".to_owned()));
}

#[test]
fn malformed_response_rejected() {
    let result = parse_muse_usage_read(fixture_value(
        r#"{"usage": {"observedAtMs": 1780000000000,
            "window": {"usedPercent": "half"}}}"#,
    ));
    assert_eq!(
        result,
        Err("Muse usage/read response is malformed".to_owned())
    );
}

#[test]
fn view_maps_observation() {
    let observation = parse_muse_usage_read(fixture_value(MUSE_USAGE_READ_FIXTURE))
        .expect("parses")
        .expect("usage present");
    let view = muse_view(
        "muse",
        "operator@example.com",
        Some(&observation),
        1_780_000_100,
    );
    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.source, UsageSource::Cache);
    assert_eq!(view.confidence, UsageConfidence::Authoritative);
    assert_eq!(view.fetched_at_epoch, 1_780_000_100);
    assert_eq!(view.account.provider_label, "Muse");
    assert_eq!(view.account.account_label, "operator@example.com");
    assert_eq!(view.account.plan_label.as_deref(), Some("Muse Pro"));
    assert_eq!(view.buckets.len(), 2);
    assert_eq!(view.status_bar_label, "Session 58% · Weekly 27%");
    assert!(view.last_error.is_none());
}

#[test]
fn view_without_observation_is_unavailable() {
    let view = muse_view("muse", "Muse account (unresolved)", None, 1_780_000_100);
    assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(view.source, UsageSource::None);
    assert_eq!(view.confidence, UsageConfidence::None);
    assert!(view.buckets.is_empty());
    assert_eq!(view.status_bar_label, "usage unavailable");
    assert!(view.last_error.is_some());
}
