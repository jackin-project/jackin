// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    assert_eq!(session.used_label.as_deref(), Some("35% used"));
    assert_eq!(session.remaining_percent, Some(65));
    let fable = buckets
        .iter()
        .find(|b| b.label == "Fable")
        .expect("Fable bucket");
    assert_eq!(fable.used_label.as_deref(), Some("150% used"));
    assert_eq!(fable.remaining_percent, Some(0));
}

#[test]
fn claude_legacy_and_limits_sources_share_one_builder() {
    let reset_at = "2026-07-03T06:59:59Z";
    let now = 1_781_300_000;
    let legacy: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        // `utilization` is percent-form here (35.0 > 1.0), matching the limits
        // `percent` field so both resolve to 35% used through the same helpers.
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

    // Same builder ⇒ identical meter, pace, reset, and severity. Only the label
    // (the model the window is scoped to) differs.
    assert_eq!(sonnet.used_label, fable.used_label);
    assert_eq!(sonnet.remaining_percent, fable.remaining_percent);
    assert_eq!(sonnet.reset_label, fable.reset_label);
    assert_eq!(sonnet.resets_at, fable.resets_at);
    assert_eq!(sonnet.pace_label, fable.pace_label);
    assert_eq!(sonnet.severity, fable.severity);
    assert_eq!(sonnet.status_slot, fable.status_slot);
}

#[test]
fn claude_limits_array_surfaces_every_scoped_model_together() {
    let reset_at = "2026-07-03T06:59:59Z";
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "limits": [
            { "kind": "session", "group": "session", "percent": 46,
              "severity": "normal", "resets_at": "2026-07-03T03:20:00Z", "scope": null },
            { "kind": "weekly_all", "group": "weekly", "percent": 36,
              "severity": "normal", "resets_at": reset_at, "scope": null },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 12,
              "severity": "normal", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Sonnet" } } },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 8,
              "severity": "normal", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Opus" } } },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 43,
              "severity": "warn", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Fable" } } }
        ]
    }))
    .expect("multi-model limits response");

    let buckets = usage.into_buckets(1_781_300_000);
    let labels: Vec<&str> = buckets.iter().map(|b| b.label.as_str()).collect();

    // Headline windows bind to their slots; every model-scoped window renders
    // as its own labelled, non-headline row.
    assert!(labels.contains(&"Session"));
    assert!(labels.contains(&"All models"));
    assert!(labels.contains(&"Sonnet"));
    assert!(labels.contains(&"Opus"));
    assert!(labels.contains(&"Fable"));
    for label in ["Sonnet", "Opus", "Fable"] {
        let b = buckets
            .iter()
            .find(|b| b.label == label)
            .unwrap_or_else(|| panic!("{label} bucket"));
        assert_eq!(b.status_slot, None, "{label} must be non-headline");
        assert!(b.remaining_percent.is_some(), "{label} must carry a meter");
    }
    // Fable carries its own (warn) severity for meter color, independent of the
    // other scoped windows.
    let fable = buckets.iter().find(|b| b.label == "Fable").expect("Fable");
    assert_eq!(fable.severity, UsageSeverity::Warn);
    assert_eq!(fable.remaining_percent, Some(57));
}

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
    // Missing / empty token yields None so the caller falls back to NeedsLogin.
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
    // A static API key has nothing to refresh.
    let api = codex_oauth_from_value(&serde_json::json!({ "OPENAI_API_KEY": "sk-x" }))
        .expect("api key credentials");
    assert!(api.refresh_token.is_none());
}

#[test]
fn unauthorized_errors_are_distinguished_from_transient() {
    for status in [401, 403] {
        assert!(usage_error_is_unauthorized(&ProviderError::from(
            ProviderHttpError::HttpStatus {
                status,
                message: format!("HTTP {status}"),
                retry_after_seconds: None,
                response_received_at_epoch: None,
            },
        )));
    }
    assert!(!usage_error_is_unauthorized(&ProviderError::from(
        ProviderHttpError::Transport("request failed: HTTP 401".to_owned()),
    )));
    assert!(!usage_error_is_unauthorized(&ProviderError::from(
        ProviderHttpError::Decode("payload mentions 403".to_owned()),
    )));
    // A rate-limit is transient, not an auth failure.
    assert!(!usage_error_is_unauthorized(&ProviderError::from(
        ProviderHttpError::HttpStatus {
            status: 429,
            message: "usage HTTP 429 rate limit".to_owned(),
            retry_after_seconds: None,
            response_received_at_epoch: None,
        },
    )));
}

#[test]
fn typed_rate_limit_preserves_retry_after_but_rendered_429_text_does_not() {
    let typed = ProviderError::from(ProviderHttpError::HttpStatus {
        status: 429,
        message: "provider response body mentions 429".to_owned(),
        retry_after_seconds: Some(37),
        response_received_at_epoch: Some(1_700_000_000),
    });
    assert!(usage_error_is_rate_limited(&typed));
    assert_eq!(typed.retry_after_seconds(), Some(37));
    assert_eq!(
        typed.rate_limit(),
        Some(ProviderRateLimit {
            retry_at_epoch: Some(1_700_000_037),
        })
    );

    for error in [
        ProviderError::from(ProviderHttpError::Transport(
            "transport failed after HTTP 429".to_owned(),
        )),
        ProviderError::from(ProviderHttpError::Decode(
            "decode failed: payload mentions 429 and Retry-After: 37".to_owned(),
        )),
    ] {
        assert!(!usage_error_is_rate_limited(&error));
        assert_eq!(error.retry_after_seconds(), None);
        assert_eq!(error.rate_limit(), None);
    }
}

#[test]
fn retry_after_accepts_delay_seconds_and_http_dates_against_response_time() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::RETRY_AFTER,
        reqwest::header::HeaderValue::from_static(" 37 "),
    );
    assert_eq!(
        retry_after_header_seconds(&headers, 1_445_412_400),
        Some(37)
    );

    headers.insert(
        reqwest::header::RETRY_AFTER,
        reqwest::header::HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"),
    );
    let delay = retry_after_header_seconds(&headers, 1_445_412_400);
    assert_eq!(delay, Some(80));
    let typed = ProviderError::from(ProviderHttpError::HttpStatus {
        status: 429,
        message: "provider HTTP 429".to_owned(),
        retry_after_seconds: delay,
        response_received_at_epoch: Some(1_445_412_400),
    });
    assert_eq!(
        typed.rate_limit(),
        Some(ProviderRateLimit {
            retry_at_epoch: Some(1_445_412_480),
        })
    );

    assert_eq!(retry_after_header_value("invalid", 1_445_412_400), None);
    assert_eq!(retry_after_header_value("37.5", 1_445_412_400), None);
    assert_eq!(retry_after_header_value("-1", 1_445_412_400), None);
    assert_eq!(
        retry_after_header_value("Wed, 21 Oct 2015 07:28:00 GMT", 1_445_412_500),
        Some(0)
    );
}

#[test]
fn claude_codename_dollar_window_is_surfaced() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": null,
        "amber_ladder": {
            "utilization": 0.0,
            "resets_at": "2026-09-02T06:59:59+00:00",
            "limit_dollars": 25000,
            "used_dollars": 5000.0
        },
        // Present but empty — must not produce a bucket.
        "omelette_promotional": { "utilization": 0.0, "limit_dollars": null }
    }))
    .expect("valid Claude OAuth usage");

    let buckets = usage.into_buckets(1_781_185_560);
    let amber = buckets
        .iter()
        .find(|bucket| bucket.label == "Amber Ladder")
        .expect("amber_ladder dollar window surfaced");
    assert_eq!(amber.used_label.as_deref(), Some("$5000.00 spent"));
    assert_eq!(amber.limit_label.as_deref(), Some("$25000.00"));
    assert_eq!(amber.remaining_percent, Some(80));
    assert_eq!(amber.status_slot, None);
    assert!(
        !buckets
            .iter()
            .any(|bucket| bucket.label.contains("omelette")),
        "a null-budget codename window must not produce a bucket"
    );
}

#[test]
fn claude_spend_disabled_is_surfaced_with_reason() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "spend": {
            "used": { "amount_minor": 7849, "currency": "SGD", "exponent": 2 },
            "limit": { "amount_minor": 26000, "currency": "SGD", "exponent": 2 },
            "percent": 30,
            "severity": "normal",
            "enabled": false,
            "disabled_reason": "out_of_credits"
        }
    }))
    .expect("valid Claude OAuth usage");

    let buckets = usage.into_buckets(1_781_185_560);
    let spend = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Spend))
        .expect("disabled spend bucket is still present");
    assert_eq!(spend.used_label.as_deref(), Some("SGD 78.49 spent"));
    assert_eq!(
        spend.pace_label.as_deref(),
        Some("disabled · out of credits")
    );
    // Headline still shows the cap context: `<used> of <limit>`.
    assert_eq!(
        spend_headline_label(&buckets).as_deref(),
        Some("SGD 78 of 260")
    );
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
    assert_eq!(
        status_bar_headline_for_surface(UsageSurface::Claude, &buckets).as_deref(),
        Some("Session 89% · Weekly 73% · SGD 78 of 260")
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
    assert_eq!(
        status_bar_headline_for_surface(UsageSurface::Claude, &buckets).as_deref(),
        Some("Session 100%"),
        "Weekly 0% and $0 spent must be omitted from the status bar"
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
        weekly.remaining_percent,
        Some(0),
        "nothing left when over cap"
    );
    assert_eq!(weekly.used_label.as_deref(), Some("150% used"));
}
