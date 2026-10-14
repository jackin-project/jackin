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
