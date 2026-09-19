// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

const SUMMARY_FIXTURE: &str = r#"{
    "email": "operator@example.com",
    "userTier": {"id": "google_ai_pro", "name": "Google AI Pro"},
    "planInfo": {"planName": "Pro"},
    "groups": [
        {"buckets": [
            {"bucketId": "gemini-5h", "remainingFraction": 0.62, "resetTime": "2026-09-17T14:00:00Z"},
            {"bucketId": "gemini-weekly", "remainingFraction": 0.81, "resetTime": "2026-09-21T00:00:00Z"}
        ]},
        {"buckets": [
            {"bucketId": "3p-5h", "remainingFraction": 0.4, "resetTime": "2026-09-17T14:00:00Z"},
            {"bucketId": "3p-weekly", "remainingFraction": 0.9, "resetTime": "2026-09-21T00:00:00Z"}
        ]}
    ]
}"#;

const LEGACY_FIXTURE: &str = r#"{
    "models": {
        "gemini-3-pro": {"displayName": "Gemini 3 Pro", "quotaInfo": {"remainingFraction": 0.7, "resetTime": "2026-09-17T14:00:00Z"}},
        "gemini-3-flash": {"displayName": "Gemini 3 Flash", "quotaInfo": {"remainingFraction": 0.2, "resetTime": "2026-09-17T14:00:00Z"}},
        "claude-opus": {"displayName": "Claude Opus", "quotaInfo": {"remainingFraction": 0.55, "resetTime": "2026-09-17T14:00:00Z"}},
        "internal-router": {"displayName": "Router", "isInternal": true, "quotaInfo": {"remainingFraction": 1.0}},
        "unlabeled": {"displayName": "", "label": "", "quotaInfo": {"remainingFraction": 0.1}},
        "available-only": {"displayName": "Available Only", "isAvailable": true}
    }
}"#;

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
    assert_eq!(buckets[0].remaining_percent, Some(62));
    assert_eq!(buckets[1].label, "Gemini · Weekly");
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[1].remaining_percent, Some(81));
    assert_eq!(buckets[2].label, "Other models · 5h");
    assert_eq!(buckets[2].status_slot, None);
    assert_eq!(buckets[2].remaining_percent, Some(40));
    assert_eq!(buckets[3].label, "Other models · Weekly");
    assert_eq!(buckets[3].remaining_percent, Some(90));
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
fn summary_absent_fraction_is_unknown_not_depleted() {
    let usage = parse_antigravity_usage_output(
        r#"{"buckets": [{"bucketId": "gemini-5h", "resetTime": "2026-09-17T14:00:00Z"}]}"#,
    )
    .expect("parses");
    assert_eq!(usage.pools.len(), 1);
    assert_eq!(usage.pools[0].remaining_percent, None);
    // …and renders as a "No data" detail row, never 0% (exhausted).
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
    assert_eq!(usage.pools[0].remaining_percent, Some(0));
    let buckets = antigravity_buckets(&usage, 1_781_728_000);
    assert_eq!(buckets[0].used_label.as_deref(), Some("100% used"));
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
    // Worst Gemini fraction (0.2) wins over 0.7; internal/empty/available-only dropped.
    assert_eq!(usage.pools.len(), 2);
    let buckets = antigravity_buckets(&usage, 1_781_728_000);
    assert_eq!(buckets.len(), 4);
    assert_eq!(buckets[0].label, "Gemini · 5h");
    assert_eq!(buckets[0].remaining_percent, Some(20));
    assert_eq!(buckets[1].label, "Other models · 5h");
    assert_eq!(buckets[1].remaining_percent, Some(55));
    // 5h-only: weekly reads "No data".
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
    assert_eq!(buckets[0].remaining_percent, Some(33));
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
    assert_eq!(bucket.remaining_percent, Some(75));

    assert!(antigravity_credits_bucket(&AntigravityCredits::default()).is_none());
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
fn gated_snapshot_reports_unsupported_without_running_usage() {
    // Cannot control the installed agy here; assert the view constructor's
    // contract directly: gated errors become typed placeholder buckets.
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
