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
