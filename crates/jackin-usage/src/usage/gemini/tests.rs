// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    // Missing tier is unknown, never consumer.
    assert!(!parse_gemini_entitlement(&serde_json::json!({})).consumer_unsupported);
}

#[test]
fn migration_action_only_for_consumer_signal() {
    let flagged = parse_gemini_entitlement(&serde_json::json!({"deprecated": true}));
    assert!(
        gemini_migration_action(Some(&flagged)).is_some_and(|action| action.contains("2026-06-18"))
    );
    // No entitlement, no migration — even past retirement: a bare OAuth
    // credential may be an already-eligible managed login.
    assert!(gemini_migration_action(None).is_none());
    let clean = parse_gemini_entitlement(&serde_json::json!({
        "currentTier": {"id": "standard", "name": "Code Assist Standard"}
    }));
    assert!(gemini_migration_action(Some(&clean)).is_none());
}

#[test]
fn error_migration_never_blind_maps() {
    let now = GEMINI_CONSUMER_OAUTH_END + 1;
    assert!(gemini_error_needs_migration(
        "HTTP 403 forbidden",
        true,
        now
    ));
    assert!(!gemini_error_needs_migration(
        "HTTP 403 forbidden",
        false,
        now
    ));
    assert!(!gemini_error_needs_migration(
        "HTTP 403 forbidden",
        true,
        GEMINI_CONSUMER_OAUTH_END - 1
    ));
    assert!(!gemini_error_needs_migration("HTTP 500", true, now));
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
    assert_eq!(buckets[0].remaining_percent, Some(75));
    assert_eq!(buckets[0].status_slot, None);
    assert_eq!(buckets[1].remaining_percent, Some(25));
    // No numbers at all: visible row, no fabricated percent.
    assert_eq!(buckets[2].remaining_percent, None);
    assert!(buckets[2].used_label.is_none());
}
