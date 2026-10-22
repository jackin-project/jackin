// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::UsageSnapshotStatus;
use jackin_usage_provider_core::parse_iso_epoch;

#[test]
fn retirement_instant_matches_deprecation_notice() {
    assert_eq!(
        parse_iso_epoch("2026-06-18T00:00:00Z"),
        Some(GEMINI_CONSUMER_OAUTH_END)
    );
    assert!(!gemini_consumer_oauth_retired(
        GEMINI_CONSUMER_OAUTH_END - 1
    ));
    assert!(gemini_consumer_oauth_retired(GEMINI_CONSUMER_OAUTH_END));
}

#[test]
fn entitlement_prefers_tier_over_plan_name() {
    let value = serde_json::json!({
        "userTier": {"id": "standard", "name": "Code Assist Standard"},
        "planInfo": {"planName": "Pro"},
        "cloudaicompanionProject": "projects/demo-123"
    });
    let entitlement = parse_gemini_entitlement(&value);
    assert_eq!(
        entitlement.tier_name.as_deref(),
        Some("Code Assist Standard")
    );
    assert_eq!(entitlement.plan_name.as_deref(), Some("Pro"));
    assert_eq!(entitlement.project.as_deref(), Some("projects/demo-123"));
    assert!(!entitlement.consumer_unsupported);
}

#[test]
fn entitlement_flags_consumer_shutdown() {
    let flagged = serde_json::json!({
        "currentTier": {"id": "individual", "name": "Code Assist Individual"},
        "consumerUnsupported": true
    });
    assert!(parse_gemini_entitlement(&flagged).consumer_unsupported);
    assert!(!parse_gemini_entitlement(&serde_json::json!({})).consumer_unsupported);
}

#[test]
fn migration_action_only_for_consumer_signal() {
    let flagged = parse_gemini_entitlement(&serde_json::json!({"deprecated": true}));
    assert!(
        gemini_migration_action(Some(&flagged)).is_some_and(|action| action.contains("2026-06-18"))
    );
    assert!(gemini_migration_action(None).is_none());
    let clean = parse_gemini_entitlement(&serde_json::json!({
        "currentTier": {"id": "standard", "name": "Code Assist Standard"}
    }));
    assert!(gemini_migration_action(Some(&clean)).is_none());
}

#[test]
fn project_quotas_never_invent_denominators() {
    let quotas = parse_gemini_project_quotas(&serde_json::json!({
        "quotas": [
            {"model": "gemini-3-pro", "metric": "requests_per_minute", "limit": 60, "used": 15},
            {"metric": "tokens_per_day", "limit": 1_000_000, "remaining": 250_000},
            {"metric": "unknown_pool"}
        ]
    }));
    assert_eq!(quotas.len(), 3);
    let buckets = gemini_quota_buckets(&quotas, 1_781_728_000);
    assert_eq!(buckets[0].label, "gemini-3-pro · Requests Per Minute");
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(buckets[0].status_slot, None);
    assert_eq!(
        buckets[0]
            .count_quota
            .map(|count| (count.used, count.limit, count.remaining, count.unit)),
        Some((
            Some(15),
            Some(60),
            None,
            jackin_protocol::control::CountQuotaUnit::Requests
        ))
    );
    assert_eq!(buckets[1].remaining_percent, None);
    assert_eq!(
        buckets[1]
            .count_quota
            .map(|count| (count.used, count.limit, count.remaining, count.unit)),
        Some((
            None,
            Some(1_000_000),
            Some(250_000),
            jackin_protocol::control::CountQuotaUnit::Unknown
        ))
    );
    assert_eq!(buckets[2].remaining_percent, None);
    assert!(buckets[2].count_quota.is_none());
    assert!(buckets[2].used_label.is_none());
    assert_eq!(buckets[2].status, UsageSnapshotStatus::Unavailable);
    assert!(
        buckets[2]
            .pace_label
            .as_deref()
            .is_some_and(|note| note.contains("No usable quota quantity"))
    );
}

#[test]
fn project_quota_counts_preserve_exact_unsigned_values() {
    let quotas = parse_gemini_project_quotas(&serde_json::json!({
        "quotas": [
            {"metric": "requests_per_day", "used": "18446744073709551615", "limit": 9007199254740993_u64},
            {"metric": "requests_per_day", "used": 18446744073709551616_u128.to_string()},
            {"metric": "requests_per_day", "used": 1.5}
        ]
    }));
    assert_eq!(quotas[0].used, Some(u64::MAX));
    assert_eq!(quotas[0].limit, Some(9_007_199_254_740_993));
    assert_eq!(quotas[1].used, None);
    assert_eq!(quotas[2].used, None);
}

#[test]
fn project_quota_invalid_fields_are_diagnostic_and_do_not_hide_valid_siblings() {
    let quotas = parse_gemini_project_quotas(&serde_json::json!({
        "quotas": [
            {"metric": "requests", "resetTime": "2026-06-01T00:00:00Z"},
            {
                "metric": "requests",
                "resetTime": "2026-06-01T00:00:00Z",
                "usage": -1,
                "limit": 1.5,
                "remaining": "18446744073709551616"
            },
            {
                "metric": "requests",
                "usage": -1,
                "used": "0",
                "limit": 1.5,
                "max": "18446744073709551615",
                "remaining": "18446744073709551616",
                "left": 0
            }
        ]
    }));
    let buckets = gemini_quota_buckets(&quotas, 1_781_728_000);

    assert_eq!(quotas[0].diagnostic, None);
    assert!(
        quotas[1]
            .diagnostic
            .as_deref()
            .is_some_and(|note| note.contains("usage")
                && note.contains("limit")
                && note.contains("remaining"))
    );
    for index in [0, 1] {
        assert_eq!(buckets[index].status, UsageSnapshotStatus::Unavailable);
        assert!(buckets[index].count_quota.is_none());
        assert!(buckets[index].pace_label.is_some());
    }
    assert!(
        buckets[0]
            .pace_label
            .as_deref()
            .is_some_and(|note| note.contains("Reset time present"))
    );
    assert!(
        buckets[1]
            .pace_label
            .as_deref()
            .is_some_and(|note| note.contains("usage")
                && note.contains("limit")
                && note.contains("remaining"))
    );

    assert_eq!(buckets[2].status, UsageSnapshotStatus::Fresh);
    assert!(
        buckets[2]
            .pace_label
            .as_deref()
            .is_some_and(|note| note.contains("usage")
                && note.contains("limit")
                && note.contains("remaining"))
    );
    assert_eq!(
        buckets[2]
            .count_quota
            .map(|count| (count.used, count.limit, count.remaining)),
        Some((Some(0), Some(u64::MAX), Some(0)))
    );
}

#[test]
fn snapshot_with_presence_routes_broker_evidence() {
    let now = 1_781_728_000;
    let view = gemini_snapshot_with_presence(
        "gemini",
        Some("Google"),
        true,
        false,
        "OAuth · configured profile",
        now,
    );
    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(view.account.provider_label, "Google");
    assert_eq!(view.focused_agent.as_deref(), Some("gemini"));
    assert_eq!(
        view.account.credential_origin.as_deref(),
        Some("OAuth · configured profile")
    );
    assert!(view.last_error.is_some());
    let view = gemini_snapshot_with_presence(
        "gemini",
        Some("Google"),
        false,
        true,
        "API key · env GEMINI_API_KEY",
        now,
    );
    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(view.account.provider_label, "Google");
    let view = gemini_snapshot_with_presence("gemini", Some("Google"), false, false, "none", now);
    assert_eq!(view.status, UsageSnapshotStatus::NeedsSecret);
}
