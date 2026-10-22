// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{PercentObservationOrigin, StatusSlot, UsageMeterSide};

#[test]
fn parses_full_observation() {
    let observation = parse_muse_usage_read(fixture_value(MUSE_USAGE_READ_FIXTURE))
        .expect("fixture parses")
        .expect("usage present");
    assert_eq!(observation.observed_at_ms, 1_780_000_000_000);
    assert_eq!(observation.tier.as_deref(), Some("Muse Pro"));
    let window = observation.window.expect("window present");
    assert_near(window.used_percent.get(), 42.5);
    assert_eq!(window.resets_at, Some(1_780_018_000));
    assert_eq!(window.window_duration_mins, Some(300));
    let weekly = observation.weekly.expect("weekly present");
    assert_near(weekly.used_percent.get(), 73.0);
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
    assert_eq!(
        buckets[0]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(142.5)
    );
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(
        buckets[0].used_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
}

#[test]
fn window_label_comes_from_duration() {
    let observation = parse_muse_usage_read(fixture_value(MUSE_USAGE_READ_FIXTURE))
        .expect("parses")
        .expect("usage present");
    let buckets = muse_buckets(&observation, 1_780_000_000);
    assert_eq!(buckets[0].label, "5 hours window");
    assert_eq!(buckets[0].used_label.as_deref(), Some("42.5% used"));
    assert_eq!(
        buckets[0]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(42.5)
    );
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(
        buckets[0].used_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(buckets[1].label, "Weekly");
    assert_eq!(
        buckets[1]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(73.0)
    );
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
fn malformed_percent_isolated_to_affected_window() {
    let result = parse_muse_usage_read(fixture_value(
        r#"{"usage": {"observedAtMs": 1780000000000,
            "tier": "Muse Pro",
            "window": {"usedPercent": -1, "resetsAtMs": 1780007200000, "windowDurationMins": 300},
            "weekly": {"usedPercent": 43.375}}}"#,
    ))
    .expect("bad window does not reject its sibling")
    .expect("usage present");
    assert!(result.invalid_window);
    assert!(!result.invalid_weekly);
    let buckets = muse_buckets(&result, 1_780_000_000);
    assert_eq!(buckets.len(), 2);
    assert_eq!(buckets[0].status, UsageSnapshotStatus::Fresh);
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(
        buckets[0]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(43.375)
    );
    assert_eq!(buckets[1].status, UsageSnapshotStatus::Unavailable);
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[1].label, "5 hours window");
    assert_eq!(buckets[1].resets_at, Some(1_780_007_200));

    let view = muse_view("muse", "operator@example.com", Some(&result), 1_780_000_100);
    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.source, UsageSource::Cache);
    assert_eq!(view.confidence, UsageConfidence::Authoritative);
    assert_eq!(view.account.account_label, "operator@example.com");
    assert_eq!(view.account.plan_label.as_deref(), Some("Muse Pro"));
    assert_eq!(view.fetched_at_epoch, 1_780_000_100);
    assert!(
        view.last_error
            .as_deref()
            .is_some_and(|error| { error.contains("Muse window usedPercent is invalid") })
    );
}

#[test]
fn wrong_type_percent_isolated_to_affected_window() {
    let result = parse_muse_usage_read(fixture_value(
        r#"{"usage": {"observedAtMs": 1780000000000,
            "window": {"usedPercent": "half"},
            "weekly": {"usedPercent": 10}}}"#,
    ))
    .expect("malformed field does not reject response")
    .expect("usage present");
    assert!(result.invalid_window);
    assert_eq!(
        result.weekly.expect("valid sibling").used_percent.get(),
        10.0
    );
}

#[test]
fn fractional_percentage_points_round_only_in_labels() {
    let observation = parse_muse_usage_read(fixture_value(
        r#"{"usage": {"observedAtMs": 1780000000000,
            "window": {"usedPercent": 43.375},
            "weekly": {"usedPercent": 0.375}}}"#,
    ))
    .expect("fractional usage parses")
    .expect("usage present");
    let buckets = muse_buckets(&observation, 1_780_000_000);
    assert_eq!(
        buckets[0]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(43.375)
    );
    assert_eq!(buckets[0].used_label.as_deref(), Some("43.38% used"));
    assert_eq!(
        buckets[1]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(0.375)
    );
    assert_eq!(buckets[1].used_label.as_deref(), Some("0.38% used"));
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&buckets[0]).1,
        Some(UsageMeterSide::Used)
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
    assert_eq!(view.status_bar_label, "Session 42.5% · Weekly 73%");
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

#[test]
fn unquantified_observations_are_unavailable_but_keep_identity_and_diagnostics() {
    for fixture in [
        r#"{"usage": {"observedAtMs": 1780000000000, "tier": "Muse Pro"}}"#,
        r#"{"usage": {"observedAtMs": 1780000000000, "tier": "Muse Pro",
            "window": {"usedPercent": -1, "resetsAtMs": 1780007200000},
            "weekly": {"usedPercent": "bad", "resetsAtMs": 1780600000000}}}"#,
        r#"{"usage": {"observedAtMs": 1780000000000, "tier": "Muse Pro",
            "window": {"resetsAtMs": 1780007200000, "windowDurationMins": 300}}}"#,
    ] {
        let observation = parse_muse_usage_read(fixture_value(fixture))
            .expect("observation structure is usable")
            .expect("usage exists");
        let view = muse_view(
            "muse",
            "operator@example.com",
            Some(&observation),
            1_780_000_100,
        );
        assert_eq!(view.status, UsageSnapshotStatus::Unavailable, "{fixture}");
        assert_eq!(view.source, UsageSource::None, "{fixture}");
        assert_eq!(view.confidence, UsageConfidence::None, "{fixture}");
        assert_eq!(
            view.account.account_label, "operator@example.com",
            "{fixture}"
        );
        assert_eq!(
            view.account.plan_label.as_deref(),
            Some("Muse Pro"),
            "{fixture}"
        );
        assert_eq!(view.fetched_at_epoch, 1_780_000_100, "{fixture}");
        assert_eq!(view.updated_label, "Unavailable", "{fixture}");
        assert!(view.last_error.is_some(), "{fixture}");
        if fixture.contains("weekly") {
            let error = view
                .last_error
                .as_deref()
                .expect("invalid windows diagnose");
            assert!(error.contains("Muse window usedPercent is invalid"));
            assert!(error.contains("Muse weekly usedPercent is invalid"));
            assert_eq!(view.buckets[0].resets_at, Some(1_780_007_200));
            assert_eq!(view.buckets[1].resets_at, Some(1_780_600_000));
        }
        if fixture.contains("windowDurationMins") {
            assert_eq!(view.buckets[0].resets_at, Some(1_780_007_200));
            assert_eq!(view.buckets[0].label, "5 hours window");
        }
    }
}
