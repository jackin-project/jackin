// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn codex_wham_relative_reset_resolves_against_now() {
    let usage: CodexUsageResponse = serde_json::from_value(serde_json::json!({
        "plan_type": "pro",
        "rate_limit": {
            "primary_window": {"used_percent": 25, "reset_after_seconds": 3_600},
            "secondary_window": {"used_percent": 50, "reset_at": 1_782_000_000}
        }
    }))
    .expect("wham relative reset decodes");
    let buckets = usage.buckets(1_781_185_560);
    let session = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Session))
        .expect("session bucket");
    assert_eq!(session.remaining_percent, Some(75));
    assert_eq!(session.resets_at, Some(1_781_185_560 + 3_600));
    // Absolute reset_at still wins when both are present.
    let weekly = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Weekly))
        .expect("weekly bucket");
    assert_eq!(weekly.resets_at, Some(1_782_000_000));
}

#[test]
fn codex_used_percent_tolerates_float_string_and_missing() {
    for (raw, expected) in [
        (serde_json::json!(63), Some(63)),
        (serde_json::json!(63.7), Some(64)),
        (serde_json::json!("41"), Some(41)),
        (serde_json::json!(140), Some(100)),
        (serde_json::json!(-3), Some(0)),
        (serde_json::json!("bogus"), None),
    ] {
        let snapshot: CodexWindowSnapshot =
            serde_json::from_value(serde_json::json!({"used_percent": raw}))
                .expect("tolerant decode");
        assert_eq!(snapshot.used_percent_clamped(), expected, "raw: {raw}");
    }
    let missing: CodexWindowSnapshot =
        serde_json::from_value(serde_json::json!({"reset_at": 1_782_000_000}))
            .expect("missing used decodes");
    assert_eq!(missing.used_percent_clamped(), None);
}

#[test]
fn codex_rpc_tolerates_missing_windows_credits_and_counts() {
    // Missing `usedPercent`, `rateLimits`, `availableCount`, and credit flags
    // all decode; unknown extra fields are ignored.
    let usage = decode_codex_rpc_usage(
        serde_json::json!({
            "rateLimits": {
                "primary": {"resetsAt": 1_782_000_000, "windowDurationMins": 300},
                "credits": {"balance": "12", "future_field": true},
                "planType": "pro",
            },
            "rateLimitsByLimitId": {},
            "rateLimitResetCredits": {"future_field": 1},
            "future_top_level": {"nested": [1, 2]},
        }),
        None,
    )
    .expect("sparse RPC decodes");
    let buckets = usage.response.buckets(1_781_185_560);
    let session = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Session))
        .expect("session bucket");
    assert_eq!(session.used_label, None);
    assert_eq!(session.remaining_percent, None);
    assert_eq!(session.resets_at, Some(1_782_000_000));
    assert!(
        buckets
            .iter()
            .all(|bucket| bucket.label != "Limit Reset Credits"),
        "zero-count credits stay hidden"
    );
    assert!(
        buckets.iter().all(|bucket| bucket.label != "Credits"),
        "flag-less credits stay hidden"
    );

    // A wholly absent `rateLimits` object still decodes to an empty snapshot.
    let empty = decode_codex_rpc_usage(serde_json::json!({}), None).expect("empty decodes");
    assert!(empty.response.buckets(1_781_185_560).is_empty());
}
