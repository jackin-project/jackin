// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

    // Monday 08:00 UTC is inside the peak window.
    let peak = quota.buckets(1_781_510_400);
    assert_eq!(peak[0].label, "Weekly");
    assert_eq!(peak[0].remaining_percent, Some(75));
    assert_eq!(peak[0].resets_at, Some(1_781_798_400));
    assert_eq!(
        peak[0].pace_label.as_deref(),
        Some("peak 1× rate · top glm-5 80% · glm-4.5 20%")
    );

    // Monday noon is off-peak.
    let off_peak = quota.buckets(1_781_524_800);
    assert_eq!(
        off_peak[0].pace_label.as_deref(),
        Some("off-peak 0.5× rate · top glm-5 80% · glm-4.5 20%")
    );
    assert_eq!(quota.plan_name().as_deref(), Some("max"));
}

#[test]
fn zai_peak_window_follows_weekday_and_hour() {
    // Monday 08:00 UTC: peak. Monday noon and Sunday 08:00: off-peak.
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
    assert_eq!(
        buckets[0].pace_label.as_deref(),
        Some("30 / 120 (90 remaining)")
    );
    assert_eq!(buckets[1].label, "Web search");
    assert_eq!(buckets[1].status_slot, None);
    assert_eq!(buckets[1].remaining_percent, Some(80));
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
