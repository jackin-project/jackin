// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

/// Fake-expiry unit case for the Codex OAuth refresh branch.
///
/// The live refresh call itself performs HTTPS against the token endpoint, so
/// no test forces a real expiry here. This pins the branch logic around it:
/// the refresh body must carry the stored refresh token (never the expired
/// access token), and a refresh reply without a usable access token must stop
/// safely instead of yielding an empty credential.
#[test]
fn expired_access_token_refreshes_through_the_stored_refresh_token() {
    let expired_access = "expired-access-token";
    let stored_refresh = "stored-refresh-token";

    let body = codex_refresh_request_body(stored_refresh);
    assert_eq!(
        body.get("grant_type").and_then(|v| v.as_str()),
        Some("refresh_token")
    );
    assert_eq!(
        body.get("refresh_token").and_then(|v| v.as_str()),
        Some(stored_refresh)
    );
    let serialized = serde_json::to_string(&body).expect("refresh body serializes");
    assert!(
        !serialized.contains(expired_access),
        "the expired access token must never leak into the refresh request"
    );

    let refreshed = serde_json::json!({
        "access_token": "fresh-access-token",
        "token_type": "Bearer",
        "expires_in": 3600,
    });
    assert_eq!(
        codex_access_token_from_response(&refreshed).as_deref(),
        Some("fresh-access-token")
    );
}

#[test]
fn refresh_reply_without_a_usable_token_stops_safely() {
    for value in [
        serde_json::json!({}),
        serde_json::json!({ "access_token": "" }),
        serde_json::json!({ "access_token": "   " }),
        serde_json::json!({ "access_token": 42 }),
    ] {
        assert!(
            codex_access_token_from_response(&value).is_none(),
            "unusable refresh reply must yield no credential: {value}"
        );
    }
}

#[test]
fn codex_credit_unlimited_state_requires_an_explicit_true_flag() {
    let expected = [
        (
            Some(true),
            Some(jackin_protocol::control::ProviderQuotaState::Unlimited),
        ),
        (Some(false), None),
        (None, None),
    ];

    for (unlimited, expected_state) in expected {
        let mut credits = serde_json::json!({
            "has_credits": true,
            "balance": "12.5"
        });
        if let Some(unlimited) = unlimited {
            credits["unlimited"] = serde_json::json!(unlimited);
        }
        let usage: CodexUsageResponse =
            serde_json::from_value(serde_json::json!({ "credits": credits }))
                .expect("valid Codex credits response");

        let buckets = usage.buckets(1_781_185_560);
        let credit = buckets
            .iter()
            .find(|bucket| bucket.label == "Credits")
            .expect("credits bucket remains visible");
        assert_eq!(credit.provider_quota_state, expected_state);
        assert_eq!(credit.limit_label.as_deref(), Some("12.50 credits"));
        assert_eq!(credit.used_percent, None);
        assert_eq!(credit.remaining_percent, None);
        assert_eq!(credit.used_percent_origin, None);
        assert_eq!(credit.remaining_percent_origin, None);
        assert!(credit.count_quota.is_none());
        assert!(credit.used_money.is_none());
        assert!(credit.limit_money.is_none());
    }
}
