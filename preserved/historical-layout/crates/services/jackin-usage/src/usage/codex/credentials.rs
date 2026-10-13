// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Codex` OAuth credential loading.

use super::super::*;

#[derive(Clone)]
pub(crate) struct CodexOAuthCredentials {
    pub(crate) access_token: String,
    pub(crate) account_id: Option<String>,
    pub(crate) account_label: Option<String>,
    /// OAuth refresh token, when present, used to re-mint a rejected
    /// `access_token` in place for a single retry (see
    /// `fetch_codex_oauth_usage_refreshing`).
    pub(crate) refresh_token: Option<String>,
}

impl std::fmt::Debug for CodexOAuthCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CodexOAuthCredentials(REDACTED)")
    }
}

#[cfg(test)]
pub(crate) fn load_codex_oauth_credentials(path: &Path) -> Option<CodexOAuthCredentials> {
    codex_oauth_from_value(&read_json_file(path)?)
}

pub(crate) fn codex_oauth_from_value(value: &serde_json::Value) -> Option<CodexOAuthCredentials> {
    if let Some(api_key) = value
        .get("OPENAI_API_KEY")
        .and_then(serde_json::Value::as_str)
        && !api_key.trim().is_empty()
    {
        return Some(CodexOAuthCredentials {
            access_token: api_key.trim().to_owned(),
            account_id: None,
            account_label: Some("OPENAI_API_KEY".to_owned()),
            // A static API key cannot be refreshed; there is nothing to re-mint.
            refresh_token: None,
        });
    }
    let tokens = value.get("tokens")?;
    let access_token = tokens
        .get("access_token")
        .or_else(|| tokens.get("accessToken"))
        .and_then(serde_json::Value::as_str)?
        .trim()
        .to_owned();
    if access_token.is_empty() {
        return None;
    }
    let account_id = tokens
        .get("account_id")
        .or_else(|| tokens.get("accountId"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let account_label = tokens
        .get("id_token")
        .or_else(|| tokens.get("idToken"))
        .and_then(serde_json::Value::as_str)
        .and_then(codex_account_label_from_id_token)
        .or_else(|| {
            tokens
                .get("account_id")
                .or_else(|| tokens.get("accountId"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        });
    let refresh_token = tokens
        .get("refresh_token")
        .or_else(|| tokens.get("refreshToken"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    Some(CodexOAuthCredentials {
        access_token,
        account_id,
        account_label,
        refresh_token,
    })
}

pub(crate) fn codex_account_label_from_id_token(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    first_string_key(&value, "email")
        .or_else(|| first_string_key(&value, "preferred_username"))
        .or_else(|| first_string_key(&value, "name"))
        .or_else(|| first_string_key(&value, "sub").map(|sub| format!("ChatGPT account {sub}")))
}
