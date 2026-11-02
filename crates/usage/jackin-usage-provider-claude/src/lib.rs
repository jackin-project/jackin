// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod spend;
mod windows;

use jackin_usage_provider_core::{Bucket, Credential, Http, ProviderError, json_text};
use serde_json::Value;

pub const API_KEY_REASON: &str = "Anthropic API keys have no subscription quota";

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const API_KEY_PREFIX: &str = "sk-ant-api";

pub async fn fetch(http: &Http, credential: Credential<'_>) -> Result<Vec<Bucket>, ProviderError> {
    let token = match credential {
        Credential::Token(token) => token.to_owned(),
        Credential::Login(login) => login_token(login)?,
    };
    if token.starts_with(API_KEY_PREFIX) {
        return Err(ProviderError::Unsupported(API_KEY_REASON));
    }
    let body = http
        .get(USAGE_URL, &token, &[("anthropic-beta", "oauth-2025-04-20")])
        .await?;
    let mut buckets = windows::buckets(&body);
    buckets.extend(spend::bucket(&body));
    Ok(buckets)
}

fn login_token(login: &str) -> Result<String, ProviderError> {
    let value: Value = serde_json::from_str(login)
        .map_err(|_| ProviderError::Decode("the Claude login file is not valid JSON"))?;
    let oauth = value.get("claudeAiOauth").ok_or(ProviderError::Decode(
        "the Claude login file has no claudeAiOauth entry",
    ))?;
    ["accessToken", "access_token"]
        .into_iter()
        .find_map(|key| oauth.get(key).and_then(json_text))
        .map(str::to_owned)
        .ok_or(ProviderError::Decode(
            "the Claude login file has no access token",
        ))
}
