// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn codex_refresh_request_body_uses_refresh_grant() {
    let body = codex_refresh_request_body("rt-abc");
    assert_eq!(body["grant_type"], "refresh_token");
    assert_eq!(body["refresh_token"], "rt-abc");
    assert_eq!(body["client_id"], CODEX_OAUTH_CLIENT_ID);
    assert!(
        !CODEX_OAUTH_CLIENT_ID.is_empty(),
        "client id must be set for the refresh grant"
    );
}

#[test]
fn codex_access_token_parsed_from_refresh_response() {
    let value = serde_json::json!({ "access_token": "  new-token  ", "token_type": "Bearer" });
    assert_eq!(
        codex_access_token_from_response(&value).as_deref(),
        Some("new-token")
    );
    assert!(codex_access_token_from_response(&serde_json::json!({})).is_none());
    assert!(codex_access_token_from_response(&serde_json::json!({ "access_token": "" })).is_none());
}

#[test]
fn codex_oauth_credentials_carry_refresh_token() {
    let value = serde_json::json!({
        "tokens": {
            "access_token": "at-1",
            "refresh_token": "rt-1",
            "account_id": "acct-1"
        }
    });
    let creds = codex_oauth_from_value(&value).expect("codex credentials");
    assert_eq!(creds.access_token, "at-1");
    assert_eq!(creds.refresh_token.as_deref(), Some("rt-1"));
    let api = codex_oauth_from_value(&serde_json::json!({ "OPENAI_API_KEY": "sk-x" }))
        .expect("api key credentials");
    assert!(api.refresh_token.is_none());
}

#[test]
fn status_bar_headline_joins_windows_and_spend() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": { "utilization": 11.0, "resets_at": "2026-06-28T16:40:00Z" },
        "seven_day": { "utilization": 27.0, "resets_at": "2026-07-03T07:00:00Z" },
        "spend": {
            "used": { "amount_minor": 7849, "currency": "SGD", "exponent": 2 },
            "limit": { "amount_minor": 26000, "currency": "SGD", "exponent": 2 },
            "percent": 30,
            "enabled": true
        }
    }))
    .expect("valid Claude OAuth usage");
    let buckets = usage.into_buckets(1_781_185_560);
    for (slot, expected_used) in [(StatusSlot::Session, 11.0), (StatusSlot::Weekly, 27.0)] {
        let window = buckets
            .iter()
            .find(|bucket| bucket.status_slot == Some(slot))
            .expect("Claude usage window");
        assert_eq!(
            window
                .used_percent
                .map(jackin_protocol::control::CanonicalUsedPercent::get),
            Some(expected_used)
        );
        assert_eq!(
            window.used_percent_origin,
            Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
        );
        assert_eq!(window.remaining_percent, None);
        assert_eq!(window.remaining_percent_origin, None);
    }
    assert_eq!(
        status_bar_headline_for_surface(UsageSurface::Claude, &buckets).as_deref(),
        Some("Session 11% · Weekly 27% · SGD 78 of 260")
    );
}

#[test]
fn status_bar_headline_drops_zero_window_and_zero_spend() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": { "utilization": 0.0, "resets_at": "2026-06-28T16:40:00Z" },
        "seven_day": { "utilization": 1.0, "resets_at": "2026-07-03T07:00:00Z" },
        "spend": {
            "used": { "amount_minor": 0, "currency": "USD", "exponent": 2 },
            "limit": { "amount_minor": 30000, "currency": "USD", "exponent": 2 },
            "percent": 0,
            "enabled": true
        }
    }))
    .expect("valid Claude OAuth usage");
    let buckets = usage.into_buckets(1_781_185_560);
    for (slot, expected_used) in [(StatusSlot::Session, 0.0), (StatusSlot::Weekly, 1.0)] {
        let window = buckets
            .iter()
            .find(|bucket| bucket.status_slot == Some(slot))
            .expect("Claude usage window");
        assert_eq!(
            window
                .used_percent
                .map(jackin_protocol::control::CanonicalUsedPercent::get),
            Some(expected_used)
        );
        assert_eq!(
            window.used_percent_origin,
            Some(PercentObservationOrigin::ProviderReported)
        );
        assert_eq!(window.remaining_percent, None);
        assert_eq!(window.remaining_percent_origin, None);
    }
    assert_eq!(
        status_bar_headline_for_surface(UsageSurface::Claude, &buckets).as_deref(),
        Some("Session 0% · Weekly 1%"),
        "$0 spend must be omitted while zero/one used points remain visible"
    );

    use jackin_protocol::control::{
        CanonicalRemainingPercent, Money, PercentObservationOrigin, QuotaBucketView, UsageSeverity,
    };
    let empty_bucket = |label: &str, slot: StatusSlot| QuotaBucketView {
        label: label.to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota: None,
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: Some(slot),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
    };
    let mut session_remaining = empty_bucket("Session", StatusSlot::Session);
    session_remaining.remaining_percent =
        Some(CanonicalRemainingPercent::new(100.0).expect("valid remaining percentage"));
    session_remaining.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    let mut weekly_remaining = empty_bucket("Weekly", StatusSlot::Weekly);
    weekly_remaining.remaining_percent =
        Some(CanonicalRemainingPercent::new(0.0).expect("valid remaining percentage"));
    weekly_remaining.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    let mut zero_spend = empty_bucket("Spend", StatusSlot::Spend);
    zero_spend.used_money = Some(Money::new(0, "USD", 2));
    zero_spend.limit_money = Some(Money::new(30_000, "USD", 2));
    let explicit_remaining = [session_remaining, weekly_remaining, zero_spend];
    assert_eq!(
        status_bar_headline_for_surface(UsageSurface::Claude, &explicit_remaining).as_deref(),
        Some("Session 100%"),
        "provider-reported zero remaining and exact zero spend stay omitted"
    );
}

#[test]
fn over_cap_window_surfaces_true_used_percent() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "seven_day": { "utilization": 150.0, "resets_at": "2026-07-03T07:00:00Z" }
    }))
    .expect("valid Claude OAuth usage");
    let buckets = usage.into_buckets(1_781_185_560);
    let weekly = buckets
        .iter()
        .find(|b| b.status_slot == Some(StatusSlot::Weekly))
        .expect("weekly bucket");
    assert_eq!(
        weekly
            .used_percent
            .map(jackin_protocol::control::CanonicalUsedPercent::get),
        Some(150.0),
        "provider-reported overage remains unbounded"
    );
    assert_eq!(
        weekly.used_percent_origin,
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(weekly.remaining_percent, None);
    assert_eq!(weekly.remaining_percent_origin, None);
    let (meter_percent, meter_side) = select_quota_meter(weekly);
    assert_eq!(
        meter_percent.map(jackin_protocol::control::MeterPercentPoints::get),
        Some(100.0)
    );
    assert_eq!(
        meter_side,
        Some(jackin_protocol::control::UsageMeterSide::Used)
    );
    assert_eq!(weekly.used_label.as_deref(), Some("150% used"));
}
