// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    HermesRuntime, UsageConfidence, UsageSnapshotStatus, UsageSource, hermes_auth_status,
    hermes_tracker_counter_buckets, hermes_view, parse_hermes_subscription,
};

/// Sanitized Portal subscription fixture: plan shape only, no secrets.
const HERMES_SUBSCRIPTION_FIXTURE: &str = r#"{
    "current": {
        "tier_id": "tier-pro",
        "tier_name": "Pro",
        "monthly_credits": "200.00",
        "credits_remaining": "173.50",
        "cycle_ends_at": "2026-10-17T00:00:00Z"
    }
}"#;

/// Sanitized local rate-limit tracker fixture: counters only, no secrets.
const HERMES_TRACKER_FIXTURE: &str = r#"{
    "windows": [
        {"model": "example-model-a", "in_flight": 1, "recent_429": 0},
        {"model": "example-model-b", "in_flight": 0, "recent_429": 2}
    ]
}"#;

fn fixture_value(fixture: &str) -> serde_json::Value {
    serde_json::from_str(fixture).expect("fixture parses")
}

#[test]
fn parses_portal_subscription() {
    let subscription = parse_hermes_subscription(fixture_value(HERMES_SUBSCRIPTION_FIXTURE))
        .expect("fixture parses")
        .expect("current present");
    assert_eq!(subscription.tier_name.as_deref(), Some("Pro"));
    assert_eq!(subscription.monthly_credits.as_deref(), Some("200.00"));
    assert_eq!(subscription.credits_remaining.as_deref(), Some("173.50"));
    assert!(subscription.cycle_ends_at.is_some_and(|epoch| epoch > 0));
}

#[test]
fn absent_current_is_no_subscription() {
    for fixture in [r"{}", r#"{"current": null}"#] {
        let subscription = parse_hermes_subscription(fixture_value(fixture)).expect("parses");
        assert!(subscription.is_none(), "fixture: {fixture}");
    }
}

#[test]
fn invalid_cycle_rejected() {
    let result = parse_hermes_subscription(fixture_value(
        r#"{"current": {"tier_name": "Pro", "cycle_ends_at": "not-a-time"}}"#,
    ));
    assert_eq!(
        result,
        Err("Hermes subscription cycle timestamp is invalid".to_owned())
    );
}

#[test]
fn cycle_end_renders_renews_never_resets() {
    let subscription = parse_hermes_subscription(fixture_value(HERMES_SUBSCRIPTION_FIXTURE))
        .expect("parses")
        .expect("current present");
    let view = super::hermes_subscription_bucket(&subscription, 1_780_000_000);
    assert_eq!(view.pace_label.as_deref(), Some("renews 2026-10-17"));
    assert_eq!(view.resets_at, None);
    assert!(view.reset_label.is_none());
}

#[test]
fn tracker_counters_yield_no_budget() {
    let counters = fixture_value(HERMES_TRACKER_FIXTURE);
    assert!(hermes_tracker_counter_buckets(&counters).is_empty());
}

#[test]
fn profile_is_exclusively_owned() {
    let runtime = HermesRuntime::for_profile("work");
    assert_eq!(runtime.profile, "work");
    assert!(runtime.exclusive);
}

#[test]
fn auth_status_mapping() {
    assert_eq!(hermes_auth_status(401), UsageSnapshotStatus::NeedsLogin);
    assert_eq!(hermes_auth_status(403), UsageSnapshotStatus::NeedsLogin);
    for code in [400, 429, 500, 503] {
        assert_eq!(
            hermes_auth_status(code),
            UsageSnapshotStatus::Error,
            "code: {code}"
        );
    }
}

#[test]
fn view_attributes_underlying_plus_portal() {
    let runtime = HermesRuntime::for_profile("work");
    let subscription = parse_hermes_subscription(fixture_value(HERMES_SUBSCRIPTION_FIXTURE))
        .expect("parses")
        .expect("current present");
    let view = hermes_view(
        "hermes",
        &runtime,
        "Nous Portal",
        "operator@example.com",
        &[],
        Some(&subscription),
        1_780_000_000,
    );
    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.source, UsageSource::ProviderApi);
    assert_eq!(view.confidence, UsageConfidence::Authoritative);
    assert_eq!(view.focused_provider.as_deref(), Some("Nous Portal"));
    assert_eq!(view.account.plan_label.as_deref(), Some("Pro"));
    assert_eq!(
        view.account.credential_origin.as_deref(),
        Some("Hermes profile 'work'")
    );
    assert_eq!(view.buckets.len(), 1);
    assert_eq!(view.buckets[0].label, "Credits");
    assert_eq!(view.buckets[0].used_label.as_deref(), Some("173.50 left"));
    assert_eq!(view.buckets[0].limit_label.as_deref(), Some("200.00 total"));
    assert!(view.last_error.is_none());
}

#[test]
fn view_without_data_is_unavailable() {
    let runtime = HermesRuntime::for_profile("work");
    let view = hermes_view(
        "hermes",
        &runtime,
        "Nous Portal",
        "operator@example.com",
        &[],
        None,
        1_780_000_000,
    );
    assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(view.source, UsageSource::None);
    assert_eq!(view.confidence, UsageConfidence::None);
    assert!(view.buckets.is_empty());
    assert_eq!(view.status_bar_label, "usage unavailable");
    assert!(view.last_error.is_some());
}
