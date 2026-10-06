// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Codex` OAuth fetch and token refresh.

use super::super::refresh::ProviderError;
use super::super::{
    CODEX_OAUTH_CLIENT_ID, CODEX_OAUTH_TOKEN_URL, Path, ProviderHttpError, get_json_bearer,
    now_epoch, provider_http_client, provider_request, retry_after_header_seconds,
    usage_error_is_unauthorized,
};

use super::{
    CodexOAuthCredentials, CodexResetCredits, CodexUsageResponse, resolve_codex_reset_credits_url,
    resolve_codex_usage_url,
};

pub(crate) fn fetch_codex_oauth_usage(
    credentials: &CodexOAuthCredentials,
    codex_home: &Path,
) -> Result<CodexUsageResponse, ProviderHttpError> {
    let mut headers = vec![(reqwest::header::USER_AGENT, "jackin-capsule/usage")];
    if let Some(account_id) = &credentials.account_id {
        headers.push((
            reqwest::header::HeaderName::from_static("chatgpt-account-id"),
            account_id.as_str(),
        ));
    }
    get_json_bearer(
        jackin_telemetry::schema::enums::ProviderName::Openai,
        "/backend-api/wham/usage",
        "Codex OAuth usage",
        &resolve_codex_usage_url(codex_home),
        &credentials.access_token,
        &headers,
    )
}

/// Body for the `refresh_token` grant. Pure so the request shape is unit-tested
/// without a live endpoint.
pub(crate) fn codex_refresh_request_body(refresh_token: &str) -> serde_json::Value {
    serde_json::json!({
        "client_id": CODEX_OAUTH_CLIENT_ID,
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "scope": "openid profile email",
    })
}

/// Extract the re-minted access token from a token-endpoint response. Pure.
pub(crate) fn codex_access_token_from_response(value: &serde_json::Value) -> Option<String> {
    value
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
}

pub(crate) fn refresh_codex_access_token(refresh_token: &str) -> Result<String, ProviderHttpError> {
    provider_request(
        jackin_telemetry::schema::enums::ProviderName::Openai,
        "POST",
        "/oauth/token",
        || {
            let client = provider_http_client().map_err(ProviderHttpError::Transport)?;
            let response = client
                .post(CODEX_OAUTH_TOKEN_URL)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .header(reqwest::header::ACCEPT, "application/json")
                .json(&codex_refresh_request_body(refresh_token))
                .send()
                .map_err(|err| {
                    ProviderHttpError::Transport(format!(
                        "Codex token refresh request failed: {err}"
                    ))
                })?;
            let response_received_at_epoch = now_epoch();
            let status = response.status();
            if !status.is_success() {
                return Err(ProviderHttpError::HttpStatus {
                    status: status.as_u16(),
                    message: format!("Codex token refresh HTTP {status}"),
                    retry_after_seconds: retry_after_header_seconds(
                        response.headers(),
                        response_received_at_epoch,
                    ),
                    response_received_at_epoch: Some(response_received_at_epoch),
                });
            }
            let value: serde_json::Value = response.json().map_err(|err| {
                ProviderHttpError::Decode(format!("Codex token refresh decode failed: {err}"))
            })?;
            codex_access_token_from_response(&value).ok_or_else(|| {
                ProviderHttpError::Decode(
                    "Codex token refresh response missing access_token".to_owned(),
                )
            })
        },
    )
}

/// Fetch Codex usage, transparently re-minting the access token once if the
/// on-disk token is rejected (HTTP 401/403).
///
/// Root cause this addresses: jackin❯ reads `auth.json` as-is, while the Codex
/// CLI refreshes that token only on its own launch — so a token that expired
/// since the last CLI run would 401 here indefinitely. The refresh is used only
/// for this read-only fetch and deliberately NOT written back to `auth.json`
/// (avoiding any risk of corrupting the operator's live credential file); the
/// CLI re-mints and persists its own copy on next launch.
pub(crate) fn fetch_codex_oauth_usage_refreshing(
    credentials: &CodexOAuthCredentials,
    codex_home: &Path,
) -> Result<CodexUsageResponse, ProviderError> {
    match fetch_codex_oauth_usage(credentials, codex_home) {
        Ok(usage) => Ok(usage),
        Err(error) => {
            let error = ProviderError::from(error);
            if !usage_error_is_unauthorized(&error) {
                return Err(error);
            }
            let Some(refresh_token) = credentials.refresh_token.as_deref() else {
                return Err(error);
            };
            let access_token =
                refresh_codex_access_token(refresh_token).map_err(ProviderError::from)?;
            let refreshed = CodexOAuthCredentials {
                access_token,
                account_id: credentials.account_id.clone(),
                account_label: credentials.account_label.clone(),
                refresh_token: credentials.refresh_token.clone(),
            };
            fetch_codex_oauth_usage(&refreshed, codex_home).map_err(ProviderError::from)
        }
    }
}

pub(crate) fn fetch_codex_oauth_reset_credits(
    credentials: &CodexOAuthCredentials,
    codex_home: &Path,
) -> Result<CodexResetCredits, ProviderHttpError> {
    let mut headers = vec![
        (reqwest::header::USER_AGENT, "jackin-capsule/usage"),
        (
            reqwest::header::HeaderName::from_static("openai-beta"),
            "codex-1",
        ),
        (
            reqwest::header::HeaderName::from_static("originator"),
            "Codex Desktop",
        ),
    ];
    if let Some(account_id) = &credentials.account_id {
        headers.push((
            reqwest::header::HeaderName::from_static("chatgpt-account-id"),
            account_id.as_str(),
        ));
    }
    let credits: CodexResetCredits = get_json_bearer(
        jackin_telemetry::schema::enums::ProviderName::Openai,
        "/backend-api/wham/usage/reset_credits",
        "Codex reset credits",
        &resolve_codex_reset_credits_url(codex_home),
        &credentials.access_token,
        &headers,
    )?;
    if credits.available_count < 0 {
        return Err(ProviderHttpError::Decode(
            "Codex reset credits invalid available count".to_owned(),
        ));
    }
    Ok(credits)
}
