// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn remaining_points(bucket: &jackin_protocol::control::QuotaBucketView) -> Option<f64> {
    bucket
        .remaining_percent
        .map(jackin_protocol::control::CanonicalRemainingPercent::get)
}

#[test]
fn agy_version_gate_rejects_pre_json() {
    assert_eq!(parse_agy_version("1.2.5"), Some((1, 2, 5)));
    assert_eq!(
        parse_agy_version("agy version 1.1.11 (build 9)"),
        Some((1, 1, 11))
    );
    assert_eq!(parse_agy_version("no version here"), None);
    assert!(!agy_version_supports_json((1, 1, 10)));
    assert!(agy_version_supports_json((1, 1, 11)));
    assert!(agy_version_supports_json((1, 2, 5)));
    assert!(agy_version_supports_json((2, 0, 0)));
}

#[test]
fn summary_pools_map_exact_bucket_ids() {
    let usage = parse_antigravity_usage_output(SUMMARY_FIXTURE).expect("summary parses");
    assert!(!usage.legacy_fallback);
    assert_eq!(usage.identity.as_deref(), Some("operator@example.com"));
    assert_eq!(usage.plan.as_deref(), Some("Google AI Pro"));
    let buckets = antigravity_buckets(&usage, 1_781_728_000);
    assert_eq!(buckets.len(), 4);
    assert_eq!(buckets[0].label, "Gemini · 5h");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(remaining_points(&buckets[0]), Some(62.0));
    assert_eq!(buckets[1].label, "Gemini · Weekly");
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(remaining_points(&buckets[1]), Some(81.0));
    assert_eq!(buckets[2].label, "Other models · 5h");
    assert_eq!(buckets[2].status_slot, None);
    assert_eq!(remaining_points(&buckets[2]), Some(40.0));
    assert_eq!(buckets[3].label, "Other models · Weekly");
    assert_eq!(remaining_points(&buckets[3]), Some(90.0));
}

#[test]
fn summary_wrapped_and_empty_wins_over_legacy() {
    let wrapped = r#"{"response": {"groups": [{"buckets": [
        {"bucketId": "gemini-5h", "remainingFraction": 0.5, "resetTime": "2026-09-17T14:00:00Z"}
    ]}]}}"#;
    let usage = parse_antigravity_usage_output(wrapped).expect("wrapped parses");
    assert!(!usage.legacy_fallback);
    assert_eq!(usage.pools.len(), 1);

    let empty = r#"{"groups": [], "models": {
        "gemini-x": {"displayName": "Gemini X", "quotaInfo": {"remainingFraction": 0.9}}
    }}"#;
    let usage = parse_antigravity_usage_output(empty).expect("empty summary parses");
    assert!(!usage.legacy_fallback);
    assert!(usage.pools.is_empty());
}

#[test]
fn summary_live_command_shape_parses() {
    let live = r#"{"command": {"data": {"groups": [{"buckets": [
        {"bucketId": "gemini-5h", "remainingFraction": 0.5, "resetTime": "2026-09-17T14:00:00Z"},
        {"bucketId": "3p-weekly", "remainingFraction": 0.9, "resetTime": "2026-09-21T00:00:00Z"}
    ]}]}}, "models": {
        "gemini-x": {"displayName": "Gemini X", "quotaInfo": {"remainingFraction": 0.1}}
    }}"#;
    let usage = parse_antigravity_usage_output(live).expect("live shape parses");
    assert!(!usage.legacy_fallback);
    assert_eq!(usage.pools.len(), 2);
    let buckets = antigravity_buckets(&usage, 1_781_728_000);
    assert_eq!(buckets.len(), 2);
    assert_eq!(buckets[0].label, "Gemini · 5h");
    assert_eq!(remaining_points(&buckets[0]), Some(50.0));
    assert_eq!(buckets[1].label, "Other models · Weekly");
    assert_eq!(remaining_points(&buckets[1]), Some(90.0));
}

#[test]
fn summary_absent_fraction_is_unknown_not_depleted() {
    let usage = parse_antigravity_usage_output(
        r#"{"buckets": [{"bucketId": "gemini-5h", "resetTime": "2026-09-17T14:00:00Z"}]}"#,
    )
    .expect("parses");
    assert_eq!(usage.pools.len(), 1);
    assert_eq!(usage.pools[0].remaining_percent, None);
    let buckets = antigravity_buckets(&usage, 1_781_728_000);
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].label, "Gemini · 5h");
    assert_eq!(buckets[0].remaining_percent, None);
    assert!(buckets[0].used_label.is_none());
    assert_eq!(buckets[0].pace_label.as_deref(), Some("No data"));
}

#[test]
fn summary_explicit_depleted_marker_stays_zero() {
    let usage = parse_antigravity_usage_output(
        r#"{"buckets": [{"bucketId": "gemini-5h", "remainingFraction": 0.0}]}"#,
    )
    .expect("parses");
    assert_eq!(usage.pools[0].remaining_percent, Some(0.0));
    let buckets = antigravity_buckets(&usage, 1_781_728_000);
    assert_eq!(buckets[0].used_label, None);
    assert_eq!(buckets[0].used_percent, None);
    assert_eq!(remaining_points(&buckets[0]), Some(0.0));
    assert_eq!(
        buckets[0].remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&buckets[0])
            .0
            .map(jackin_protocol::control::MeterPercentPoints::get),
        Some(0.0)
    );
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&buckets[0]).1,
        Some(jackin_protocol::control::UsageMeterSide::Remaining)
    );
}

#[test]
fn version_gate_status_needs_secret_except_predates_json() {
    assert_eq!(
        antigravity_version_error_status("Antigravity CLI unavailable: not found"),
        UsageSnapshotStatus::NeedsSecret
    );
    assert_eq!(
        antigravity_version_error_status("Antigravity CLI version was not recognized"),
        UsageSnapshotStatus::NeedsSecret
    );
    assert_eq!(
        antigravity_version_error_status(
            "Antigravity CLI 1.0.0 predates JSON usage (needs >= 1.1.11)"
        ),
        UsageSnapshotStatus::Unsupported
    );
}

#[test]
fn legacy_collapses_to_worst_fraction_per_family() {
    let usage = parse_antigravity_usage_output(LEGACY_FIXTURE).expect("legacy parses");
    assert!(usage.legacy_fallback);
    assert!(usage.identity.is_none());
    assert_eq!(usage.pools.len(), 2);
    let buckets = antigravity_buckets(&usage, 1_781_728_000);
    assert_eq!(buckets.len(), 4);
    assert_eq!(buckets[0].label, "Gemini · 5h");
    assert_eq!(remaining_points(&buckets[0]), Some(20.0));
    assert_eq!(buckets[1].label, "Other models · 5h");
    assert!(
        (remaining_points(&buckets[1]).expect("remaining points") - 55.0).abs() < 1e-12,
        "0.55 source fraction should remain 55 percentage points"
    );
    assert_eq!(buckets[2].label, "Gemini · Weekly");
    assert_eq!(buckets[2].pace_label.as_deref(), Some("No data"));
    assert_eq!(buckets[2].remaining_percent, None);
    assert_eq!(buckets[3].label, "Other models · Weekly");
    assert_eq!(buckets[3].pace_label.as_deref(), Some("No data"));
}

#[test]
fn availability_only_never_becomes_quota() {
    let usage = parse_antigravity_usage_output(
        r#"{"models": {"m": {"displayName": "M", "isAvailable": true, "available": 1.0}}}"#,
    )
    .expect("parses");
    assert!(usage.pools.is_empty());
    assert!(antigravity_buckets(&usage, 1_781_728_000).is_empty());
}

#[test]
fn unknown_window_pool_kept_as_detail_row() {
    let usage = parse_antigravity_usage_output(
        r#"{"pools": [{"id": "gemini-mystery", "remainingPercent": 33}]}"#,
    )
    .expect("parses");
    assert_eq!(usage.pools.len(), 1);
    let buckets = antigravity_buckets(&usage, 1_781_728_000);
    assert_eq!(buckets[0].label, "Quota · gemini-mystery");
    assert_eq!(buckets[0].status_slot, None);
    assert_eq!(remaining_points(&buckets[0]), Some(33.0));
}

#[test]
fn non_json_rejected() {
    parse_antigravity_usage_output("not json").unwrap_err();
    parse_antigravity_credits_output("not json").unwrap_err();
}

#[test]
fn credits_money_only_with_explicit_minor_units() {
    let minor = parse_antigravity_credits_output(
        r#"{"credits": {"used_minor": 1250, "limit_minor": 5000, "currency": "USD", "exponent": 2}}"#,
    )
    .expect("parses");
    let bucket = antigravity_credits_bucket(&minor).expect("bucket");
    assert_eq!(bucket.status_slot, Some(StatusSlot::Spend));
    assert_eq!(bucket.used_money.as_ref().map(Money::major), Some(12.5));

    let major = parse_antigravity_credits_output(
        r#"{"credits": {"balance": 37.5, "limit": 50, "unit": "credits"}}"#,
    )
    .expect("parses");
    let bucket = antigravity_credits_bucket(&major).expect("bucket");
    assert_eq!(bucket.status_slot, None);
    assert!(bucket.used_money.is_none());
    assert_eq!(remaining_points(&bucket), Some(75.0));
    assert_eq!(
        bucket.remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::Derived)
    );

    let ambiguous_amount =
        parse_antigravity_credits_output(r#"{"credits": {"remaining": 37.5, "unit": "credits"}}"#)
            .expect("amount parses");
    let bucket = antigravity_credits_bucket(&ambiguous_amount).expect("bucket");
    assert_eq!(remaining_points(&bucket), None);
    assert_eq!(bucket.remaining_percent_origin, None);

    assert!(antigravity_credits_bucket(&AntigravityCredits::default()).is_none());
}

#[test]
fn summary_preserves_fractional_used_and_remaining_observations() {
    use jackin_protocol::control::PercentObservationOrigin::ProviderReported;

    let usage = parse_antigravity_usage_output(
        r#"{"buckets": [{"bucketId": "gemini-5h", "usedPercent": 37.625, "remainingPercent": 62.375}]}"#,
    )
    .expect("parses fractional source percentages");
    let bucket = antigravity_buckets(&usage, 1_781_728_000).remove(0);

    assert_eq!(
        usage.pools[0].used_percent,
        Some(37.625),
        "raw provider-used percentage keeps its precision"
    );
    assert_eq!(
        bucket
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(37.625)
    );
    assert_eq!(bucket.used_percent_origin, Some(ProviderReported));
    assert_eq!(remaining_points(&bucket), Some(62.375));
    assert_eq!(bucket.remaining_percent_origin, Some(ProviderReported));

    let subpoint = parse_antigravity_usage_output(
        r#"{"buckets": [{"bucketId": "gemini-5h", "usedPercent": 0.375, "remainingFraction": 0.8}]}"#,
    )
    .expect("parses subpoint percent");
    assert_eq!(subpoint.pools[0].used_percent, Some(0.375));

    let numeric = parse_antigravity_usage_output(
        r#"{"buckets":[{"bucketId":"gemini-5h","usedPercent":0.375,"remainingFraction":0.00375,"resetTime":"2026-07-17T14:00:00Z"}]}"#,
    )
    .expect("summary JSON-number percentages parse");
    let numeric_string = parse_antigravity_usage_output(
        r#"{"buckets":[{"bucketId":"gemini-5h","usedPercent":"0.375","remainingFraction":"0.00375","resetTime":"2026-07-17T14:00:00Z"}]}"#,
    )
    .expect("summary numeric-string percentages parse");
    let numeric_pool = &numeric.pools[0];
    let string_pool = &numeric_string.pools[0];
    assert_eq!(numeric_pool.used_percent, string_pool.used_percent);
    assert_eq!(
        numeric_pool.remaining_percent,
        string_pool.remaining_percent
    );
    assert_eq!(
        numeric_pool.used_percent_origin,
        string_pool.used_percent_origin
    );
    assert_eq!(
        numeric_pool.remaining_percent_origin,
        string_pool.remaining_percent_origin
    );
    assert_eq!(numeric_pool.reset_at, string_pool.reset_at);
    assert_eq!(
        string_pool.reset_at,
        jackin_usage_provider_core::parse_iso_epoch("2026-07-17T14:00:00Z")
    );
    let string_bucket = antigravity_buckets(&numeric_string, 1_781_728_000).remove(0);
    assert_eq!(
        string_bucket
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(0.375)
    );
    assert_eq!(remaining_points(&string_bucket), Some(0.375));
    assert_eq!(string_bucket.used_percent_origin, Some(ProviderReported));
    assert_eq!(
        string_bucket.remaining_percent_origin,
        Some(ProviderReported)
    );
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&string_bucket)
            .0
            .map(jackin_protocol::control::MeterPercentPoints::get),
        Some(0.375)
    );
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&string_bucket).1,
        Some(jackin_protocol::control::UsageMeterSide::Used)
    );

    let untyped = parse_antigravity_usage_output(
        r#"{"buckets": [{"bucketId": "gemini-5h", "used": 37.5, "remaining": 62.5}]}"#,
    )
    .expect("parses untyped values as a detail row");
    assert_eq!(untyped.pools[0].used_percent, None);
    assert_eq!(untyped.pools[0].remaining_percent, None);
}

#[test]
fn snapshot_status_needs_quota_signal_not_just_parse() {
    let empty = AntigravityUsage::default();
    assert_eq!(
        antigravity_snapshot_status(Some(&empty), None),
        UsageSnapshotStatus::Stale
    );
    assert_eq!(
        antigravity_snapshot_status(None, None),
        UsageSnapshotStatus::Stale
    );
    let with_pools = parse_antigravity_usage_output(SUMMARY_FIXTURE).expect("summary parses");
    assert_eq!(
        antigravity_snapshot_status(Some(&with_pools), None),
        UsageSnapshotStatus::Fresh
    );
    let credits = antigravity_credits_bucket(
        &parse_antigravity_credits_output(
            r#"{"credits": {"balance": 37.5, "limit": 50, "unit": "credits"}}"#,
        )
        .expect("credits parse"),
    )
    .expect("credits bucket");
    assert_eq!(
        antigravity_snapshot_status(Some(&empty), Some(&credits)),
        UsageSnapshotStatus::Fresh
    );
}

#[test]
fn utilization_keeps_exact_value_without_guessing_its_unit() {
    for utilization in [0.0, 0.375, 1.0, 43.375, 150.0, -0.25] {
        let input = serde_json::json!({
            "buckets": [{"bucketId": "gemini-5h", "utilization": utilization}]
        })
        .to_string();
        let usage = parse_antigravity_usage_output(&input).expect("usage parses");
        let source = &usage.pools[0].unclassified_utilizations[0];
        assert_eq!(source.value.as_f64(), Some(utilization));
        assert_eq!(usage.pools[0].used_percent, None);
        assert_eq!(usage.pools[0].remaining_percent, None);

        let bucket = antigravity_buckets(&usage, 1_781_728_000).remove(0);
        assert_eq!(bucket.used_percent, None);
        assert_eq!(bucket.remaining_percent, None);
        assert!(
            bucket.pace_label.as_deref().is_some_and(|note| {
                note.contains("utilization") && note.contains("unit unknown")
            })
        );
    }

    let explicit = parse_antigravity_usage_output(
        r#"{"buckets": [{"bucketId": "gemini-5h", "usedPercent": 0.375, "remainingFraction": 0.8}]}"#,
    )
    .expect("explicit fields parse");
    assert_eq!(explicit.pools[0].used_percent, Some(0.375));
    assert_eq!(explicit.pools[0].remaining_percent, Some(80.0));
    assert!(explicit.pools[0].unclassified_utilizations.is_empty());

    let legacy = parse_antigravity_usage_output(
        r#"{"models":{"gemini-x":{"displayName":"Gemini X","quotaInfo":{"utilization":-0.375,"resetTime":"2026-07-17T14:00:00Z"}}}}"#,
    )
    .expect("legacy utilization parses");
    assert_eq!(legacy.pools[0].used_percent, None);
    assert_eq!(legacy.pools[0].remaining_percent, None);
    assert_eq!(
        legacy.pools[0].unclassified_utilizations[0].value.as_f64(),
        Some(-0.375)
    );
    assert!(
        legacy.pools[0].unclassified_utilizations[0]
            .reset_at
            .is_some()
    );
    let legacy_bucket = antigravity_buckets(&legacy, 1_781_728_000).remove(0);
    assert!(legacy_bucket.pace_label.as_deref().is_some_and(|note| {
        note.contains("-0.375") && note.contains("unit unknown") && note.contains("Resets")
    }));
}

#[test]
fn legacy_family_rank_uses_unrounded_used_authority_for_overages() {
    for models in [
        r#"{"gemini-a": {"displayName":"Gemini A","quotaInfo":{"usedPercent":100.125}},"gemini-b":{"displayName":"Gemini B","quotaInfo":{"usedPercent":150.0}}}"#,
        r#"{"gemini-b":{"displayName":"Gemini B","quotaInfo":{"usedPercent":150.0}},"gemini-a": {"displayName":"Gemini A","quotaInfo":{"usedPercent":100.125}}}"#,
    ] {
        let input = format!("{{\"models\":{models}}}");
        let usage = parse_antigravity_usage_output(&input).expect("legacy usage parses");
        assert_eq!(usage.pools.len(), 1);
        assert_eq!(usage.pools[0].used_percent, Some(150.0));
        assert_eq!(usage.pools[0].remaining_percent, None);
    }
}

#[test]
fn legacy_family_rank_prefers_used_when_reported_sides_disagree() {
    let usage = parse_antigravity_usage_output(
        r#"{"models": {
            "gemini-low-remaining": {"displayName":"Gemini A", "quotaInfo":{"usedPercent":30.0,"remainingPercent":5.0}},
            "gemini-high-used": {"displayName":"Gemini B", "quotaInfo":{"usedPercent":40.0,"remainingPercent":80.0}}
        }}"#,
    )
    .expect("legacy usage parses");
    assert_eq!(usage.pools.len(), 1);
    assert_eq!(usage.pools[0].used_percent, Some(40.0));
    assert_eq!(usage.pools[0].remaining_percent, Some(80.0));
}

#[test]
fn legacy_family_rank_retains_numeric_string_percentages() {
    let usage = parse_antigravity_usage_output(
        r#"{"models": {
            "gemini-a": {"displayName":"Gemini A", "quotaInfo":{"usedPercent":"60.375","remainingFraction":"0.1","resetTime":"2026-06-01T00:00:00Z"}},
            "gemini-b": {"displayName":"Gemini B", "quotaInfo":{"usedPercent":"70.625","remainingFraction":"0.9","resetTime":"2026-07-17T14:00:00Z"}}
        }}"#,
    )
    .expect("legacy numeric-string percentages parse");

    assert_eq!(usage.pools.len(), 1);
    let pool = &usage.pools[0];
    assert_eq!(pool.used_percent, Some(70.625));
    assert_eq!(pool.remaining_percent, Some(90.0));
    assert_eq!(
        pool.reset_at,
        jackin_usage_provider_core::parse_iso_epoch("2026-07-17T14:00:00Z")
    );

    let bucket = antigravity_buckets(&usage, 1_781_728_000).remove(0);
    assert_eq!(
        bucket
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(70.625)
    );
    assert_eq!(remaining_points(&bucket), Some(90.0));
    assert_eq!(
        bucket.used_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        bucket.remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&bucket)
            .0
            .map(jackin_protocol::control::MeterPercentPoints::get),
        Some(70.625)
    );
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&bucket).1,
        Some(jackin_protocol::control::UsageMeterSide::Used)
    );
}

#[test]
fn legacy_utilization_diagnostics_keep_each_reset_without_changing_selected_reset() {
    let input = r#"{"models": {
        "gemini-a-util": {"displayName":"Gemini A", "quotaInfo":{"utilization":0.375,"resetTime":"2026-06-01T00:00:00Z"}},
        "gemini-b-util": {"displayName":"Gemini B", "quotaInfo":{"utilization":1.25,"resetTime":"2026-07-17T14:00:00Z"}},
        "gemini-z-canonical": {"displayName":"Gemini Z", "quotaInfo":{"usedPercent":30.0,"resetTime":"2026-08-17T14:00:00Z"}}
    }}"#;
    let usage = parse_antigravity_usage_output(input).expect("legacy usage parses");
    assert_eq!(usage.pools.len(), 1);
    let pool = &usage.pools[0];
    assert_eq!(pool.used_percent, Some(30.0));
    assert_eq!(
        pool.reset_at,
        jackin_usage_provider_core::parse_iso_epoch("2026-08-17T14:00:00Z")
    );
    assert_eq!(pool.unclassified_utilizations.len(), 2);
    assert_eq!(pool.unclassified_utilizations[0].source, "gemini-a-util");
    assert_eq!(
        pool.unclassified_utilizations[0].reset_at,
        jackin_usage_provider_core::parse_iso_epoch("2026-06-01T00:00:00Z")
    );
    assert_eq!(pool.unclassified_utilizations[1].source, "gemini-b-util");
    assert_eq!(
        pool.unclassified_utilizations[1].reset_at,
        jackin_usage_provider_core::parse_iso_epoch("2026-07-17T14:00:00Z")
    );

    let bucket = antigravity_buckets(&usage, 1_781_728_000).remove(0);
    let note = bucket.pace_label.expect("diagnostic note");
    assert!(note.contains(
        "gemini-a-util utilization 0.375 (unit unknown; not ranked as a percentage; Resets now)"
    ));
    assert!(note.contains(
        "gemini-b-util utilization 1.25 (unit unknown; not ranked as a percentage; Resets in "
    ));
    assert_eq!(bucket.resets_at, pool.reset_at);
}

fn assert_numeric_string_percentages_preserve_observations() {
    use jackin_protocol::control::PercentObservationOrigin::ProviderReported;

    for (field, numeric, string, expected, side) in [
        (
            "usedPercent",
            "43.375",
            "43.375",
            43.375,
            jackin_protocol::control::UsageMeterSide::Used,
        ),
        (
            "usedPercent",
            "0.375",
            "0.375",
            0.375,
            jackin_protocol::control::UsageMeterSide::Used,
        ),
        (
            "remainingPercent",
            "57.125",
            "57.125",
            57.125,
            jackin_protocol::control::UsageMeterSide::Remaining,
        ),
        (
            "remainingFraction",
            "0.5",
            "0.5",
            50.0,
            jackin_protocol::control::UsageMeterSide::Remaining,
        ),
        (
            "remainingFraction",
            "0.00375",
            "0.00375",
            0.375,
            jackin_protocol::control::UsageMeterSide::Remaining,
        ),
    ] {
        let numeric_input = format!(r#"{{"credits":{{"{field}":{numeric}}}}}"#);
        let string_input = format!(r#"{{"credits":{{"{field}":"{string}"}}}}"#);
        let numeric = parse_antigravity_credits_output(&numeric_input)
            .expect("JSON-number percentage parses");
        let string = parse_antigravity_credits_output(&string_input)
            .expect("numeric-string percentage parses");
        let numeric_bucket = antigravity_credits_bucket(&numeric).expect("JSON-number bucket");
        let string_bucket = antigravity_credits_bucket(&string).expect("numeric-string bucket");

        assert_eq!(
            numeric_bucket
                .used_percent
                .map(jackin_protocol::control::CanonicalUsedPercent::get),
            string_bucket
                .used_percent
                .map(jackin_protocol::control::CanonicalUsedPercent::get)
        );
        assert_eq!(
            numeric_bucket
                .remaining_percent
                .map(jackin_protocol::control::CanonicalRemainingPercent::get),
            string_bucket
                .remaining_percent
                .map(jackin_protocol::control::CanonicalRemainingPercent::get)
        );
        assert_eq!(
            numeric_bucket.used_percent_origin,
            string_bucket.used_percent_origin
        );
        assert_eq!(
            numeric_bucket.remaining_percent_origin,
            string_bucket.remaining_percent_origin
        );
        assert_eq!(
            jackin_usage_provider_core::select_quota_meter(&string_bucket)
                .0
                .map(jackin_protocol::control::MeterPercentPoints::get),
            Some(expected)
        );
        assert_eq!(
            jackin_usage_provider_core::select_quota_meter(&string_bucket).1,
            Some(side)
        );
        match side {
            jackin_protocol::control::UsageMeterSide::Used => {
                assert_eq!(string_bucket.used_percent_origin, Some(ProviderReported));
                assert_eq!(string_bucket.remaining_percent_origin, None);
            }
            jackin_protocol::control::UsageMeterSide::Remaining => {
                assert_eq!(string_bucket.used_percent_origin, None);
                assert_eq!(
                    string_bucket.remaining_percent_origin,
                    Some(ProviderReported)
                );
            }
        }
    }
}

#[test]
fn credits_percentage_only_evidence_survives_without_money() {
    use jackin_protocol::control::PercentObservationOrigin::ProviderReported;

    let used = parse_antigravity_credits_output(r#"{"credits":{"usedPercent":43.375}}"#)
        .expect("used-only credits parse");
    let used_bucket = antigravity_credits_bucket(&used).expect("used-only bucket");
    assert_eq!(
        used_bucket
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(43.375)
    );
    assert_eq!(used_bucket.used_percent_origin, Some(ProviderReported));
    assert_eq!(used_bucket.remaining_percent, None);
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&used_bucket).1,
        Some(jackin_protocol::control::UsageMeterSide::Used)
    );

    let remaining = parse_antigravity_credits_output(r#"{"credits":{"remainingPercent":57.125}}"#)
        .expect("remaining-only credits parse");
    let remaining_bucket = antigravity_credits_bucket(&remaining).expect("remaining-only bucket");
    assert_eq!(
        remaining_bucket
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(57.125)
    );
    assert_eq!(
        remaining_bucket.remaining_percent_origin,
        Some(ProviderReported)
    );
    assert_eq!(remaining_bucket.used_percent, None);
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&remaining_bucket).1,
        Some(jackin_protocol::control::UsageMeterSide::Remaining)
    );

    let both = parse_antigravity_credits_output(
        r#"{"credits":{"usedPercent":43.375,"remainingPercent":99.125}}"#,
    )
    .expect("independent sides parse");
    let both_bucket = antigravity_credits_bucket(&both).expect("both-sides bucket");
    assert_eq!(
        both_bucket
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(43.375)
    );
    assert_eq!(both_bucket.used_percent_origin, Some(ProviderReported));
    assert_eq!(
        both_bucket
            .remaining_percent
            .map(jackin_protocol::control::CanonicalRemainingPercent::get),
        Some(99.125)
    );
    assert_eq!(both_bucket.remaining_percent_origin, Some(ProviderReported));
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&both_bucket).1,
        Some(jackin_protocol::control::UsageMeterSide::Used)
    );

    assert_numeric_string_percentages_preserve_observations();

    let unknown = parse_antigravity_credits_output(r#"{"credits":{"utilization":0.375}}"#)
        .expect("unclassified utilization parses");
    let unknown_bucket = antigravity_credits_bucket(&unknown).expect("diagnostic bucket");
    assert_eq!(unknown_bucket.used_percent, None);
    assert_eq!(unknown_bucket.remaining_percent, None);
    assert!(
        unknown_bucket
            .pace_label
            .as_deref()
            .is_some_and(|note| { note.contains("0.375") && note.contains("unit unknown") })
    );
}

#[test]
fn invalid_credit_percentages_do_not_create_quota_or_drop_valid_sides() {
    for malformed in [
        r#"{"credits":{}}"#,
        r#"{"credits":{"resetTime":"2026-07-17T14:00:00Z"}}"#,
        r#"{"credits":{"usedPercent":"not-a-number"}}"#,
        r#"{"credits":{"usedPercent":true}}"#,
        r#"{"credits":{"usedPercent":{}}}"#,
        r#"{"credits":{"usedPercent":[]}}"#,
        r#"{"credits":{"usedPercent":null}}"#,
        r#"{"credits":{"usedPercent":-1}}"#,
        r#"{"credits":{"usedPercent":"-1"}}"#,
        r#"{"credits":{"usedPercent":"NaN"}}"#,
        r#"{"credits":{"usedPercent":"1e999"}}"#,
        r#"{"credits":{"remainingPercent":"Infinity"}}"#,
        r#"{"credits":{"remainingPercent":null}}"#,
        r#"{"credits":{"remainingFraction":-0.5}}"#,
        r#"{"credits":{"remainingFraction":"-0.5"}}"#,
        r#"{"credits":{"remainingFraction":1e307}}"#,
        r#"{"credits":{"remainingFraction":"1e307"}}"#,
    ] {
        let parsed = parse_antigravity_credits_output(malformed).expect("JSON parses");
        assert!(antigravity_credits_bucket(&parsed).is_none(), "{malformed}");
    }

    let invalid_used = parse_antigravity_credits_output(
        r#"{"credits":{"usedPercent":"bad","remainingPercent":"57.125"}}"#,
    )
    .expect("valid remaining sibling survives");
    let bucket = antigravity_credits_bucket(&invalid_used).expect("remaining-only bucket");
    assert_eq!(bucket.used_percent, None);
    assert_eq!(remaining_points(&bucket), Some(57.125));
    assert_eq!(
        bucket.remaining_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&bucket).1,
        Some(jackin_protocol::control::UsageMeterSide::Remaining)
    );

    let invalid_remaining = parse_antigravity_credits_output(
        r#"{"credits":{"usedPercent":"0.375","remainingFraction":"bad"}}"#,
    )
    .expect("valid used sibling survives");
    let bucket = antigravity_credits_bucket(&invalid_remaining).expect("used-only bucket");
    assert_eq!(
        bucket
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(0.375)
    );
    assert_eq!(bucket.remaining_percent, None);
    assert_eq!(
        bucket.used_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        jackin_usage_provider_core::select_quota_meter(&bucket).1,
        Some(jackin_protocol::control::UsageMeterSide::Used)
    );
}

#[test]
fn gated_snapshot_reports_unsupported_without_running_usage() {
    let view = antigravity_status_view(
        "antigravity",
        None,
        1_781_728_000,
        UsageSnapshotStatus::Unsupported,
        "Antigravity CLI 1.0.0 predates JSON usage (needs >= 1.1.11)",
    );
    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(view.account.provider_label, "Antigravity");
    assert_eq!(view.buckets.len(), 1);
    assert!(
        view.last_error
            .is_some_and(|error| error.contains("1.1.11"))
    );
}
