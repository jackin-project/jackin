// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod windows;

use jackin_usage_provider_core::{Bucket, Http, ProviderError, json_text};
use serde_json::Value;

pub const API_KEY_REASON: &str = "OpenAI API keys have no subscription quota";

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

pub async fn fetch(
    http: &Http,
    login: &str,
    chatgpt_account_id: &str,
    now: i64,
) -> Result<Vec<Bucket>, ProviderError> {
    let token = access_token(login)?;
    let body = http
        .get(
            USAGE_URL,
            &token,
            &[("chatgpt-account-id", chatgpt_account_id)],
        )
        .await?;
    windows::buckets(&body, now)
}

fn access_token(login: &str) -> Result<String, ProviderError> {
    let value: Value = serde_json::from_str(login)
        .map_err(|_| ProviderError::Decode("the Codex login file is not valid JSON"))?;
    let token = value
        .get("tokens")
        .and_then(|tokens| tokens.get("access_token"))
        .and_then(json_text);
    if let Some(token) = token {
        return Ok(token.to_owned());
    }
    match value.get("OPENAI_API_KEY").and_then(json_text) {
        Some(_) => Err(ProviderError::Unsupported(API_KEY_REASON)),
        None => Err(ProviderError::Decode(
            "the Codex login file has no access token",
        )),
    }
}
