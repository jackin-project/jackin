// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Cursor` session REST transport.

use super::CURSOR_SESSION_BASE;
use jackin_usage_provider_core::provider_http_client;

pub fn cursor_session_cookie(user_id: &str, token: &str) -> String {
    format!("WorkosCursorSessionToken={user_id}%3A%3A{token}")
}

pub fn cursor_rest_get(
    user_id: &str,
    token: &str,
    path: &str,
) -> Result<serde_json::Value, String> {
    let client = provider_http_client()?;
    let response = client
        .get(format!("{CURSOR_SESSION_BASE}{path}"))
        .header(
            reqwest::header::COOKIE,
            cursor_session_cookie(user_id, token),
        )
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .map_err(|error| format!("Cursor REST {path} request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Cursor REST {path} HTTP {status}"));
    }
    response
        .json::<serde_json::Value>()
        .map_err(|error| format!("Cursor REST {path} decode failed: {error}"))
}
