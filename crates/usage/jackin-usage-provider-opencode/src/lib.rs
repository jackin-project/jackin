// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Bucket, Credential, Http, ProviderError, json_epoch, json_number, json_text,
};
use serde_json::Value;

const USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";
const ENTRY: &str = "opencode-go";
const WINDOWS: [(&str, i64); 3] = [
    ("rolling", 300 * 60),
    ("weekly", 10_080 * 60),
    ("monthly", 43_200 * 60),
];

pub async fn fetch(http: &Http, credential: Credential<'_>) -> Result<Vec<Bucket>, ProviderError> {
    let key = match credential {
        Credential::Token(key) => key.to_owned(),
        Credential::Login(login) => login_key(login)?,
    };
    let body = http.get(USAGE_URL, &key, &[]).await?;
    Ok(WINDOWS
        .iter()
        .filter_map(|(name, seconds)| {
            let window = body.pointer(&format!("/usage/{name}"))?;
            let used = window.get("percent").and_then(json_number)?;
            Some(
                Bucket::timed(None, Some(*seconds), None, name)
                    .used(Some(used))
                    .resets(window.get("resetsAt").and_then(json_epoch)),
            )
        })
        .collect())
}

fn login_key(login: &str) -> Result<String, ProviderError> {
    let value: Value = serde_json::from_str(login)
        .map_err(|_| ProviderError::Decode("the OpenCode login file is not valid JSON"))?;
    let entry = value
        .get(ENTRY)
        .filter(|entry| entry.get("type").and_then(json_text) == Some("api"))
        .ok_or(ProviderError::Decode(
            "the OpenCode login file has no opencode-go key",
        ))?;
    entry
        .get("key")
        .and_then(json_text)
        .map(str::to_owned)
        .ok_or(ProviderError::Decode(
            "the OpenCode login file has no opencode-go key",
        ))
}
