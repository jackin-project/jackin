// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn usage_detail_presentation_stale_keeps_buckets_and_one_detail() {
    let bucket = presentation_bucket(
        "Weekly",
        Some(57),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Stale,
    );
    let view = detail_view(
        vec![bucket],
        Some("upstream 503"),
        UsageSnapshotStatus::Stale,
    );
    let presentation = usage_detail_presentation(&view);
    let detail_rows: Vec<&jackin_protocol::control::UsageDetailRow> = presentation
        .rows
        .iter()
        .filter(|r| r.kind == jackin_protocol::control::UsageDetailRowKind::Detail)
        .collect();
    assert_eq!(detail_rows.len(), 1, "exactly one Detail row");
    assert_eq!(detail_rows[0].display_label, "upstream 503");
    // The Detail row is last and the last-good bucket survives.
    assert_eq!(
        presentation.rows.last().map(|r| r.row_id.as_str()),
        Some("detail")
    );
    assert!(
        presentation.rows.iter().any(|r| r.row_id == "bucket:0"),
        "bucket retained under stale"
    );
}

#[test]
fn usage_detail_presentation_amp_daily_and_bounds() {
    let mut daily = presentation_bucket(
        "Daily",
        Some(61),
        Some(StatusSlot::Daily),
        UsageSnapshotStatus::Fresh,
    );
    daily.reset_label = Some("Resets daily".to_owned());
    let mut individual = presentation_bucket("Credits", None, None, UsageSnapshotStatus::Fresh);
    individual.limit_label = Some("$4.76".to_owned());
    let view = detail_view(vec![daily, individual], None, UsageSnapshotStatus::Fresh);
    let presentation = usage_detail_presentation(&view);
    let by_id = |id: &str| {
        presentation
            .rows
            .iter()
            .find(|r| r.row_id == id)
            .expect("row")
    };
    let daily_row = by_id("bucket:0");
    assert_eq!(daily_row.display_label, "61% left · Resets daily");
    // No fabricated exact reset timestamp or paid-plan label.
    assert!(!daily_row.display_label.contains('('));
    // Credit bound stays in source order after Daily.
    assert_eq!(by_id("bucket:1").display_label, "$4.76");
}

#[test]
fn usage_detail_presentation_grok_bounds_no_provider_path() {
    let mut weekly = presentation_bucket(
        "Weekly",
        Some(72),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    weekly.reset_label = Some("Resets in 3d".to_owned());
    let mut prepaid = presentation_bucket(
        "Extra usage credits",
        None,
        None,
        UsageSnapshotStatus::Fresh,
    );
    prepaid.limit_label = Some("$25.00".to_owned());
    let view = detail_view(vec![weekly, prepaid], None, UsageSnapshotStatus::Fresh);
    let view = FocusedUsageView {
        account: FocusedAccountHeader {
            plan_label: Some("SuperGrok".to_owned()),
            ..view.account.clone()
        },
        ..view
    };
    let presentation = usage_detail_presentation(&view);
    assert_eq!(
        presentation
            .rows
            .iter()
            .find(|row| row.row_id == "plan")
            .map(|row| row.display_label.as_str()),
        Some("SuperGrok")
    );
    assert_eq!(
        presentation.rows.last().map(|r| r.display_label.as_str()),
        Some("$25.00")
    );
}

#[test]
fn quota_pace_label_appends_runout_when_behind_pace() {
    // time_left=53%, delta=-5; elapsed=470, used=52; 48*470/52=433.85 -> 434s -> "7m"; 434 < 530.
    assert_eq!(
        quota_pace_label(Some(48), Some(10_530), Some(1_000), 10_000).expect("pace"),
        "5% in deficit · Runs out in 7m"
    );
    // Weekly-realistic 7-day window: 48*284401/52 = 262524s ~ 3d; 262524 < 320399.
    assert_eq!(
        quota_pace_label(Some(48), Some(320_399), Some(604_800), 0).expect("pace"),
        "5% in deficit · Runs out in 3d"
    );
}

#[test]
fn quota_pace_label_no_runout_when_ahead_of_pace() {
    // run-out would be 90*400/10 = 3600 >= 600 -> no segment.
    assert_eq!(
        quota_pace_label(Some(90), Some(600), Some(1_000), 0).expect("pace"),
        "30% in reserve"
    );
}

#[test]
fn quota_pace_label_no_runout_when_nothing_used() {
    // used == 0 -> returns without dividing (no division by zero).
    assert_eq!(
        quota_pace_label(Some(100), Some(500), Some(1_000), 0).expect("pace"),
        "50% in reserve"
    );
}

#[test]
fn quota_pace_label_no_runout_at_window_start() {
    // elapsed == 0 -> no segment even though delta = -40.
    assert_eq!(
        quota_pace_label(Some(60), Some(1_000), Some(1_000), 0).expect("pace"),
        "40% in deficit"
    );
}

#[test]
fn quota_pace_label_runout_iff_behind_clock_boundary() {
    // reset_at=500, window=1000, now=0.
    // delta=0 -> On pace; run-out 50*500/50=500, not strictly < 500 -> bare.
    assert_eq!(
        quota_pace_label(Some(50), Some(500), Some(1_000), 0).expect("pace"),
        "On pace"
    );
    // delta=+1 (ahead, in band); 51*500/49=520.4 -> 520 >= 500 -> bare.
    assert_eq!(
        quota_pace_label(Some(51), Some(500), Some(1_000), 0).expect("pace"),
        "On pace"
    );
    // delta=-1 (behind, in band); 49*500/51=480.4 -> 480s -> "8m"; 480 < 500.
    assert_eq!(
        quota_pace_label(Some(49), Some(500), Some(1_000), 0).expect("pace"),
        "On pace · Runs out in 8m"
    );
    // delta=-2 (band edge); 48*500/52=461.5 -> 462s -> "7m".
    assert_eq!(
        quota_pace_label(Some(48), Some(500), Some(1_000), 0).expect("pace"),
        "On pace · Runs out in 7m"
    );
    // delta=-3 (first deficit token); 47*500/53=443.4 -> 443s -> "7m".
    assert_eq!(
        quota_pace_label(Some(47), Some(500), Some(1_000), 0).expect("pace"),
        "3% in deficit · Runs out in 7m"
    );
}

#[test]
fn quota_pace_label_runout_depleted_bucket() {
    // used=100, elapsed=500, run-out=0 < 500 -> trivially precedes reset.
    assert_eq!(
        quota_pace_label(Some(0), Some(500), Some(1_000), 0).expect("pace"),
        "50% in deficit · Runs out in <1m"
    );
}

#[test]
fn quota_pace_label_exact_projection_precedes_reset_before_rounding() {
    // Exact 49*536/51 = 514.98… < 515; display rounding is 515 (would fail if
    // rounded seconds were compared to reset seconds).
    assert_eq!(
        quota_pace_label(Some(49), Some(10_515), Some(1_051), 10_000).expect("pace"),
        "On pace · Runs out in 8m"
    );
}

#[test]
fn quota_pace_label_exact_clock_equality_ignores_float_drift() {
    // 7*1000 == 70*100 -> projection reaches reset exactly -> no run-out segment.
    let label = quota_pace_label(Some(7), Some(70), Some(1_000), 0).expect("pace");
    assert!(!label.contains("Runs out"), "unexpected run-out: {label}");
}

#[test]
fn claude_limits_inactive_flag_does_not_gate_rendering() {
    // Live responses send `is_active: false` on headline limits that still
    // carry quota — the flag must never suppress a bucket.
    let response: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": null,
        "seven_day": null,
        "limits": [
            {"kind": "session", "percent": 10, "is_active": false,
             "resets_at": "2026-09-17T10:00:00Z"},
            {"kind": "weekly_all", "percent": 42, "is_active": false,
             "resets_at": "2026-09-24T10:00:00Z"},
        ]
    }))
    .expect("inactive limits decode");
    let buckets = response.into_buckets(1_781_185_560);
    let session = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Session))
        .expect("session bucket despite is_active false");
    assert_eq!(session.label, "Session");
    assert_eq!(session.remaining_percent, Some(90));
    let weekly = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Weekly))
        .expect("weekly bucket despite is_active false");
    assert_eq!(weekly.label, "All models");
    assert_eq!(weekly.remaining_percent, Some(58));
}

#[test]
fn claude_scope_restriction_error_is_explicit() {
    let forbidden = ProviderError::from(ProviderHttpError::HttpStatus {
        status: 403,
        message: "Claude OAuth usage HTTP 403 Forbidden".to_owned(),
        retry_after_seconds: None,
        response_received_at_epoch: None,
    });
    assert!(claude_error_is_scope_restriction(&forbidden));
    assert!(!claude_error_is_scope_restriction(&ProviderError::from(
        ProviderHttpError::Transport("HTTP 403 insufficient_scope".to_owned()),
    )));
    assert!(!claude_error_is_scope_restriction(&ProviderError::from(
        ProviderHttpError::HttpStatus {
            status: 401,
            message: "Claude OAuth usage HTTP 401 Unauthorized".to_owned(),
            retry_after_seconds: None,
            response_received_at_epoch: None,
        },
    )));
    assert!(!claude_error_is_scope_restriction(&ProviderError::from(
        "Claude OAuth usage request failed: connection reset".to_owned(),
    )));
    assert_eq!(
        claude_provider_error_label(
            Some(&forbidden),
            Some(&ProviderError::from("cli boom".to_owned()))
        )
        .as_deref(),
        Some("Claude token lacks usage scope (inference-only); quota unavailable")
    );
    // Non-scope errors pass through verbatim, OAuth first.
    assert_eq!(
        claude_provider_error_label(
            Some(&ProviderError::from("oauth boom".to_owned())),
            Some(&ProviderError::from("cli boom".to_owned())),
        )
        .as_deref(),
        Some("oauth boom")
    );
    assert_eq!(
        claude_provider_error_label(None, Some(&ProviderError::from("cli boom".to_owned())))
            .as_deref(),
        Some("cli boom")
    );
    assert_eq!(claude_provider_error_label(None, None), None);
}

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
