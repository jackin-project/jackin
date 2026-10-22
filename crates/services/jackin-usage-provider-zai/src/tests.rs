// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::UsageSnapshotStatus;

#[test]
fn zai_credit_limit_carries_rate_note_and_model_breakdown() {
    let quota: ZaiQuotaResponse = serde_json::from_value(serde_json::json!({
        "code": 200,
        "success": true,
        "data": {
            "level": "max",
            "limits": [{
                "type": "CREDIT_LIMIT",
                "unit": 6,
                "number": 1,
                "usage": 10000,
                "currentValue": 2500,
                "remaining": 7500,
                "percentage": 25,
                "nextResetTime": 1_781_798_400_000_i64,
                "usageDetails": [
                    { "modelCode": "glm-5", "usage": 2000 },
                    { "modelCode": "glm-4.5", "usage": 500 }
                ]
            }]
        }
    }))
    .expect("valid Z.AI credit quota");

    let peak = quota.buckets(1_781_510_400);
    assert_eq!(peak[0].label, "Weekly");
    assert_eq!(peak[0].remaining_percent, None);
    assert_eq!(
        peak[0]
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(25.0)
    );
    assert_eq!(
        peak[0].used_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        peak[0]
            .count_quota
            .map(|count| (count.used, count.limit, count.remaining)),
        Some((Some(2_500), Some(10_000), Some(7_500)))
    );
    assert_eq!(peak[0].resets_at, Some(1_781_798_400));
    assert_eq!(
        peak[0].pace_label.as_deref(),
        Some("peak 1× rate · top glm-5 80% · glm-4.5 20%")
    );

    let off_peak = quota.buckets(1_781_524_800);
    assert_eq!(
        off_peak[0].pace_label.as_deref(),
        Some("off-peak 0.5× rate · top glm-5 80% · glm-4.5 20%")
    );
    assert_eq!(quota.plan_name().as_deref(), Some("max"));
}

#[test]
fn zai_peak_window_follows_weekday_and_hour() {
    assert!(zai_is_peak(1_781_510_400));
    assert!(!zai_is_peak(1_781_524_800));
    assert!(!zai_is_peak(1_781_424_000));
    assert_eq!(zai_credit_rate_note(1_781_510_400), "peak 1× rate");
    assert_eq!(zai_credit_rate_note(1_781_424_000), "off-peak 0.5× rate");
}

#[test]
fn zai_time_limit_splits_mcp_and_web_search() {
    let quota: ZaiQuotaResponse = serde_json::from_value(serde_json::json!({
        "code": 200,
        "success": true,
        "data": {
            "limits": [
                {
                    "type": "TIME_LIMIT",
                    "unit": 5,
                    "number": 60,
                    "usage": 120,
                    "currentValue": 30,
                    "remaining": 90
                },
                {
                    "type": "TIME_LIMIT",
                    "unit": 1,
                    "number": 30,
                    "usage": 500,
                    "currentValue": 100,
                    "remaining": 400
                }
            ]
        }
    }))
    .expect("valid Z.AI time quota");
    let buckets = quota.buckets(1_781_185_560);
    assert_eq!(buckets[0].label, "MCP");
    assert_eq!(buckets[0].status_slot, None);
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(
        buckets[0].pace_label.as_deref(),
        Some("30 used · 120 limit · 90 remaining")
    );
    assert_eq!(
        buckets[0]
            .count_quota
            .map(|count| (count.used, count.limit, count.remaining)),
        Some((Some(30), Some(120), Some(90)))
    );
    assert_eq!(buckets[1].label, "Web search");
    assert_eq!(buckets[1].status_slot, None);
    assert_eq!(buckets[1].remaining_percent, None);
    assert_eq!(
        buckets[1]
            .count_quota
            .map(|count| (count.used, count.limit, count.remaining)),
        Some((Some(100), Some(500), Some(400)))
    );
}

#[test]
fn zai_team_scope_parses_selectors() {
    let scope = zai_team_scope_from(Some("2"), Some("org-1"), Some("proj-9"));
    assert!(scope.active());
    assert_eq!(scope.query(), Some("2"));
    assert_eq!(scope.organization.as_deref(), Some("org-1"));
    assert_eq!(scope.project.as_deref(), Some("proj-9"));

    let empty = zai_team_scope_from(None, Some("  "), None);
    assert!(!empty.active());
    assert_eq!(empty.query(), None);
}

#[test]
fn zai_model_note_skips_empty_breakdown() {
    let bare: ZaiLimitRaw = serde_json::from_value(serde_json::json!({
        "type": "TOKENS_LIMIT",
        "unit": 5,
        "number": 300,
        "percentage": 10
    }))
    .expect("bare limit");
    assert_eq!(zai_model_note(&bare), None);

    let blank: ZaiLimitRaw = serde_json::from_value(serde_json::json!({
        "type": "TOKENS_LIMIT",
        "unit": 5,
        "number": 300,
        "percentage": 10,
        "usageDetails": [{ "modelCode": "  ", "usage": 0 }]
    }))
    .expect("blank breakdown");
    assert_eq!(zai_model_note(&blank), None);
}

#[test]
fn zai_count_fields_preserve_full_width_and_parse_decimal_strings_independently() {
    let quota: ZaiQuotaResponse = serde_json::from_value(serde_json::json!({
        "data": { "limits": [{
            "type": "TOKENS_LIMIT",
            "unit": 5,
            "number": 300,
            "currentValue": 9007199254740993_u64,
            "usage": "18446744073709551615",
            "remaining": 0
        }] }
    }))
    .expect("wide Z.AI count values");

    let bucket = quota.buckets(1_781_185_560).remove(0);
    assert_eq!(bucket.status, UsageSnapshotStatus::Fresh);
    assert_eq!(
        bucket
            .count_quota
            .map(|count| (count.used, count.limit, count.remaining)),
        Some((Some(9_007_199_254_740_993), Some(u64::MAX), Some(0)))
    );
}

#[test]
fn zai_malformed_count_does_not_discard_valid_siblings_or_create_fresh_empty_quota() {
    let quota: ZaiQuotaResponse = serde_json::from_value(serde_json::json!({
        "data": { "limits": [
            {
                "type": "TOKENS_LIMIT",
                "unit": 5,
                "number": 300,
                "currentValue": "bad",
                "usage": 9007199254740993_u64,
                "remaining": "0"
            },
            {
                "type": "TOKENS_LIMIT",
                "unit": 5,
                "number": 300,
                "currentValue": -1,
                "usage": 1.5,
                "remaining": "18446744073709551616",
                "percentage": -1
            }
        ] }
    }))
    .expect("malformed counts are isolated at field boundaries");

    let buckets = quota.buckets(1_781_185_560);
    assert_eq!(buckets.len(), 2);
    assert_eq!(buckets[0].status, UsageSnapshotStatus::Fresh);
    assert_eq!(
        buckets[0]
            .count_quota
            .map(|count| (count.used, count.limit, count.remaining)),
        Some((None, Some(9_007_199_254_740_993), Some(0)))
    );
    assert!(
        buckets[0]
            .pace_label
            .as_deref()
            .is_some_and(|note| note.contains("invalid count field: currentValue"))
    );
    assert_eq!(buckets[1].status, UsageSnapshotStatus::Unavailable);
    assert!(buckets[1].count_quota.is_none());
    assert!(
        buckets[1]
            .pace_label
            .as_deref()
            .is_some_and(|note| note.contains("invalid count fields"))
    );
}
