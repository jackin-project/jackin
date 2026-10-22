// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn claude_account_email_reads_oauth_account_metadata() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        r#"{"oauthAccount":{"emailAddress":"alexey@example.com"}}"#,
    )
    .expect("write");
    assert_eq!(
        load_claude_account_email(&path).as_deref(),
        Some("alexey@example.com")
    );

    let empty = dir.path().join("empty.json");
    fs::write(&empty, r#"{"oauthAccount":{}}"#).expect("write");
    assert_eq!(load_claude_account_email(&empty), None);

    let none = dir.path().join("none.json");
    fs::write(&none, "{}").expect("write");
    assert_eq!(load_claude_account_email(&none), None);
}

#[test]
fn claude_oauth_usage_decodes_live_api_body() {
    let body = r#"{
            "five_hour": {"utilization": 12, "resets_at": "2026-06-25T19:00:00Z"},
            "seven_day": {"utilization": 34, "resets_at": "2026-06-26T14:00:00Z"},
            "seven_day_oauth_apps": null,
            "seven_day_sonnet": {"utilization": 5, "resets_at": "2026-06-26T14:00:00Z"},
            "seven_day_opus": null,
            "seven_day_cowork": null,
            "seven_day_omelette": null,
            "amber_ladder": null, "cinder_cove": null, "iguana_necktie": null,
            "omelette_promotional": null, "tangelo": null,
            "extra_usage": {"is_enabled": false, "monthly_limit": 0, "used_credits": 0,
                "utilization": 0, "currency": "USD", "decimal_places": 2,
                "disabled_reason": "x", "daily": null, "weekly": null},
            "limits": [{"kind": "x", "group": "x", "percent": 0, "severity": "x",
                "resets_at": "x", "scope": null, "is_active": false}],
            "spend": null
        }"#;
    let parsed: ClaudeOAuthUsageResponse =
        serde_json::from_str(body).expect("decode live OAuth usage body");
    assert!(parsed.five_hour.is_some());
    assert!(parsed.seven_day.is_some());
    assert!(parsed.seven_day_sonnet.is_some());
}

#[test]
fn claude_oauth_response_maps_windows_to_buckets() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": { "utilization": 0.84, "resets_at": "2026-06-11T15:12:00Z" },
        "seven_day": { "utilization": 0.78, "resets_at": "2026-06-12T14:26:00Z" },
        "seven_day_sonnet": { "utilization": 0.02, "resets_at": "2026-06-12T14:26:00Z" },
        "seven_day_routines": { "utilization": 0.0 },
        "extra_usage": {
            "is_enabled": true,
            "monthly_limit": 26000.0,
            "used_credits": 7849.0,
            "utilization": 30.0,
            "currency": "SGD",
            "decimal_places": 2
        }
    }))
    .expect("valid Claude OAuth usage");

    let buckets = usage.into_buckets(1_781_185_560);

    assert_eq!(buckets[0].label, "Session");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[0].remaining_percent, None);
    assert_eq!(used_points(&buckets[0]), Some(0.84));
    assert_eq!(buckets[0].used_label.as_deref(), Some("0.84% used"));
    assert_eq!(
        buckets[0].reset_label.as_deref(),
        Some(
            reset_label(
                parse_iso_epoch("2026-06-11T15:12:00Z").expect("session reset"),
                1_781_185_560,
            )
            .as_str()
        )
    );
    assert_eq!(buckets[1].label, "Weekly");
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[1].remaining_percent, None);
    assert_eq!(used_points(&buckets[1]), Some(0.78));
    assert!(buckets.iter().any(|bucket| bucket.label == "Sonnet"));
    assert_eq!(
        used_points(
            buckets
                .iter()
                .find(|bucket| bucket.label == "Sonnet")
                .expect("Sonnet")
        ),
        Some(0.02)
    );
    assert!(
        buckets
            .iter()
            .find(|bucket| bucket.label == "Sonnet")
            .is_some_and(|bucket| bucket.status_slot.is_none())
    );
    assert!(
        buckets
            .iter()
            .find(|bucket| bucket.label == "Sonnet")
            .is_some_and(|bucket| bucket.pace_label.is_some())
    );
    assert!(buckets.iter().any(|bucket| bucket.label == "Daily Routines"
        && bucket.remaining_percent.is_none()
        && used_points(bucket) == Some(0.0)
        && bucket.pace_label.is_none()
        && bucket.status_slot.is_none()));
    assert!(!buckets.iter().any(|bucket| bucket.label == "Opus"));
    let extra = buckets
        .iter()
        .find(|bucket| bucket.label == "Extra usage")
        .expect("extra usage bucket");
    assert_eq!(extra.status_slot, Some(StatusSlot::Spend));
    assert_eq!(extra.remaining_percent, None);
    assert_eq!(used_points(extra), Some(30.0));
    assert_eq!(extra.used_label.as_deref(), Some("SGD 78.49 spent"));
    assert_eq!(extra.limit_label.as_deref(), Some("SGD 260.00"));
    assert_eq!(extra.pace_label.as_deref(), Some("30% used"));
    assert_eq!(
        extra.used_money.as_ref().map(Money::to_string).as_deref(),
        Some("SGD 78.49")
    );
    assert_eq!(
        extra
            .used_money
            .as_ref()
            .map(Money::format_compact)
            .as_deref(),
        Some("SGD 78")
    );
}

#[test]
fn claude_spend_object_preferred_and_scaled() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": null,
        "seven_day": null,
        "extra_usage": {
            "is_enabled": true,
            "monthly_limit": 30000.0,
            "used_credits": 5331.0,
            "utilization": 17.77,
            "currency": "USD",
            "decimal_places": 2
        },
        "spend": {
            "used": { "amount_minor": 5331, "currency": "USD", "exponent": 2 },
            "limit": { "amount_minor": 30000, "currency": "USD", "exponent": 2 },
            "percent": 18,
            "severity": "normal",
            "enabled": true
        }
    }))
    .expect("valid Claude OAuth usage");

    let buckets = usage.into_buckets(1_781_185_560);
    let spend = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Spend))
        .expect("spend bucket");
    assert_eq!(spend.used_label.as_deref(), Some("$53.31 spent"));
    assert_eq!(spend.limit_label.as_deref(), Some("$300.00"));
    assert_eq!(spend.pace_label.as_deref(), Some("18% used"));
    assert_eq!(spend.remaining_percent, None);
    assert_eq!(used_points(spend), Some(18.0));
    assert_eq!(spend.severity, UsageSeverity::Normal);

    assert_eq!(
        spend_headline_label(&buckets).as_deref(),
        Some("$53 of 300")
    );
}

#[test]
fn claude_spend_preserves_fractional_reported_percent() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "spend": {
            "used": { "amount_minor": 1_234, "currency": "USD", "exponent": 2 },
            "limit": { "amount_minor": 10_000, "currency": "USD", "exponent": 2 },
            "percent": 37.125,
            "enabled": true
        }
    }))
    .expect("valid fractional Claude spend");

    let spend = usage
        .into_buckets(1_781_185_560)
        .into_iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Spend))
        .expect("spend bucket");
    assert_eq!(used_points(&spend), Some(37.125));
    assert_eq!(
        spend.used_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(spend.pace_label.as_deref(), Some("37.13% used"));
    assert_eq!(
        spend.used_money.as_ref().map(|money| money.amount_minor),
        Some(1_234)
    );
}

#[test]
fn claude_oauth_limits_array_surfaces_fable_and_all_models() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": null,
        "seven_day": null,
        "seven_day_sonnet": null,
        "seven_day_opus": null,
        "seven_day_cowork": null,
        "limits": [
            { "kind": "session", "group": "session", "percent": 7,
              "severity": "normal", "resets_at": "2026-07-03T03:19:59Z",
              "scope": null, "is_active": false },
            { "kind": "weekly_all", "group": "weekly", "percent": 28,
              "severity": "normal", "resets_at": "2026-07-03T07:00:00Z",
              "scope": null, "is_active": false },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 35,
              "severity": "warn", "resets_at": "2026-07-03T06:59:59Z",
              "scope": { "model": { "id": null, "display_name": "Fable" },
                         "surface": null },
              "is_active": true }
        ]
    }))
    .expect("valid Claude OAuth limits-array response");

    let buckets = usage.into_buckets(1_781_300_000);

    let session = buckets
        .iter()
        .find(|b| b.status_slot == Some(StatusSlot::Session))
        .expect("session bucket from limits");
    assert_eq!(session.label, "Session");
    assert_eq!(session.remaining_percent, None);
    assert_eq!(used_points(session), Some(7.0));
    assert_eq!(session.used_label.as_deref(), Some("7% used"));

    let all_models = buckets
        .iter()
        .find(|b| b.status_slot == Some(StatusSlot::Weekly))
        .expect("weekly slot from limits");
    assert_eq!(all_models.label, "All models");
    assert_eq!(all_models.remaining_percent, None);
    assert_eq!(used_points(all_models), Some(28.0));

    let fable = buckets
        .iter()
        .find(|b| b.label == "Fable")
        .expect("Fable model-scoped bucket");
    assert_eq!(fable.remaining_percent, None);
    assert_eq!(used_points(fable), Some(35.0));
    assert_eq!(fable.used_label.as_deref(), Some("35% used"));
    assert_eq!(fable.status_slot, None);
    assert_eq!(fable.severity, UsageSeverity::Warn);
    assert!(fable.resets_at.is_some());

    assert!(buckets.iter().all(|b| b.label != "Weekly"));
    assert!(buckets.iter().all(|b| b.label != "Sonnet"));
    assert!(buckets.iter().all(|b| b.label != "Opus"));
    assert!(buckets.iter().all(|b| b.label != "Daily Routines"));
}

#[test]
fn claude_oauth_limits_array_skips_unnamed_scoped_window() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "limits": [
            { "kind": "weekly_scoped", "group": "weekly", "percent": 40,
              "severity": "normal", "resets_at": "2026-07-03T06:59:59Z",
              "scope": { "model": { "id": null, "display_name": null } },
              "is_active": true }
        ]
    }))
    .expect("valid limits response");
    let buckets = usage.into_buckets(1_781_300_000);
    assert!(buckets.is_empty(), "unnamed scoped window must be omitted");
}

#[test]
fn claude_oauth_limits_array_backfills_missing_legacy_windows() {
    let reset_at = "2026-07-03T06:59:59Z";
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": { "utilization": 22.0, "resets_at": "2026-07-03T03:19:59Z" },
        "seven_day": { "utilization": 44.0, "resets_at": reset_at },
        "seven_day_sonnet": { "utilization": 55.0, "resets_at": reset_at },
        "limits": [
            { "kind": "session", "group": "session", "percent": 7,
              "severity": "normal", "resets_at": "2026-07-03T03:19:59Z",
              "scope": null },
            { "kind": "future_shape", "group": "weekly", "percent": 99,
              "severity": "normal", "resets_at": reset_at, "scope": null }
        ]
    }))
    .expect("mixed limits/legacy response");

    let buckets = usage.into_buckets(1_781_300_000);

    let session_buckets = buckets
        .iter()
        .filter(|b| b.status_slot == Some(StatusSlot::Session))
        .count();
    assert_eq!(session_buckets, 1, "Session duplicate must be skipped");
    let weekly = buckets
        .iter()
        .find(|b| b.status_slot == Some(StatusSlot::Weekly))
        .expect("legacy Weekly backfill");
    assert_eq!(weekly.label, "Weekly");
    assert_eq!(weekly.remaining_percent, None);
    assert_eq!(used_points(weekly), Some(44.0));
    let sonnet = buckets
        .iter()
        .find(|b| b.label == "Sonnet")
        .expect("legacy Sonnet backfill");
    assert_eq!(sonnet.remaining_percent, None);
    assert_eq!(used_points(sonnet), Some(55.0));
}

#[test]
fn claude_oauth_limits_percent_accepts_string_float_and_over_cap() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "limits": [
            { "kind": "session", "group": "session", "percent": "35.4",
              "severity": "normal", "resets_at": null, "scope": null },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 150.0,
              "severity": "danger", "resets_at": null,
              "scope": { "model": { "display_name": "Fable" } } }
        ]
    }))
    .expect("lenient percent response");

    let buckets = usage.into_buckets(1_781_300_000);
    let session = buckets
        .iter()
        .find(|b| b.status_slot == Some(StatusSlot::Session))
        .expect("session bucket");
    assert_eq!(session.used_label.as_deref(), Some("35.4% used"));
    assert_eq!(session.remaining_percent, None);
    assert_eq!(used_points(session), Some(35.4));
    let fable = buckets
        .iter()
        .find(|b| b.label == "Fable")
        .expect("Fable bucket");
    assert_eq!(fable.used_label.as_deref(), Some("150% used"));
    assert_eq!(fable.remaining_percent, None);
    assert_eq!(used_points(fable), Some(150.0));
}

#[test]
fn claude_explicit_limit_percent_is_always_percentage_points() {
    let values = [0.0, 0.375, 1.0, 12.34, 43.375, 150.0];
    let limits = values
        .iter()
        .map(|percent| {
            serde_json::json!({
                "kind": "session",
                "percent": percent,
                "severity": "normal"
            })
        })
        .collect::<Vec<_>>();
    let usage: ClaudeOAuthUsageResponse =
        serde_json::from_value(serde_json::json!({ "limits": limits }))
            .expect("explicit Claude percentages");

    let buckets = usage.into_buckets(1_781_300_000);
    assert_eq!(buckets.len(), values.len());
    for (bucket, expected) in buckets.iter().zip(values) {
        assert_eq!(used_points(bucket), Some(expected));
        assert_eq!(
            bucket.used_percent_origin,
            Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
        );
    }
    assert_eq!(buckets[1].used_label.as_deref(), Some("0.38% used"));
    assert_eq!(buckets[2].used_label.as_deref(), Some("1% used"));
    assert_eq!(buckets[3].used_label.as_deref(), Some("12.34% used"));
}

#[test]
fn claude_extra_usage_fractional_percent_is_points_and_missing_percent_is_not_inferred() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "extra_usage": {
            "is_enabled": true,
            "monthly_limit": 30000.0,
            "used_credits": 5331.0,
            "utilization": 0.375,
            "currency": "USD",
            "decimal_places": 2
        }
    }))
    .expect("fractional extra-usage percentage");
    let bucket = usage
        .into_buckets(1_781_300_000)
        .into_iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Spend))
        .expect("extra-usage bucket");
    assert_eq!(used_points(&bucket), Some(0.375));
    assert_eq!(
        bucket.used_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
    );

    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "extra_usage": {
            "is_enabled": true,
            "monthly_limit": 30000.0,
            "used_credits": 5331.0,
            "currency": "USD",
            "decimal_places": 2
        }
    }))
    .expect("extra-usage amounts without a percent");
    let bucket = usage
        .into_buckets(1_781_300_000)
        .into_iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Spend))
        .expect("extra-usage bucket");
    assert_eq!(bucket.used_percent, None);
    assert_eq!(bucket.used_percent_origin, None);
}

#[test]
fn claude_legacy_and_limits_sources_share_one_builder() {
    let reset_at = "2026-07-03T06:59:59Z";
    let now = 1_781_300_000;
    let legacy: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "seven_day_sonnet": { "utilization": 35.0, "resets_at": reset_at }
    }))
    .expect("legacy response");
    let limits: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "limits": [
            { "kind": "weekly_scoped", "group": "weekly", "percent": 35,
              "severity": "normal", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Fable" } }, "is_active": true }
        ]
    }))
    .expect("limits response");

    let sonnet = legacy
        .into_buckets(now)
        .into_iter()
        .find(|b| b.label == "Sonnet")
        .expect("legacy Sonnet bucket");
    let fable = limits
        .into_buckets(now)
        .into_iter()
        .find(|b| b.label == "Fable")
        .expect("limits Fable bucket");

    assert_eq!(sonnet.used_label, fable.used_label);
    assert_eq!(sonnet.remaining_percent, None);
    assert_eq!(fable.remaining_percent, None);
    assert_eq!(used_points(&sonnet), used_points(&fable));
    assert_eq!(sonnet.reset_label, fable.reset_label);
    assert_eq!(sonnet.resets_at, fable.resets_at);
    assert_eq!(sonnet.pace_label, fable.pace_label);
    assert_eq!(sonnet.severity, fable.severity);
    assert_eq!(sonnet.status_slot, fable.status_slot);
}
