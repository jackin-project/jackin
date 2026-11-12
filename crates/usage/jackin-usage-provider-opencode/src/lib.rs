// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Auth, Bucket, Credential, Http, ProviderError, json_epoch, json_number, json_text,
};
use serde_json::Value;

const USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";
const WINDOWS: [(&str, i64); 3] = [
    ("rolling", 300 * 60),
    ("weekly", 10_080 * 60),
    ("monthly", 43_200 * 60),
];

pub async fn fetch(
    http: &Http,
    credential: Credential<'_>,
    now: i64,
) -> Result<Vec<Bucket>, ProviderError> {
    let (key, org) = match credential {
        Credential::Token(key) => (key.to_owned(), None),
        Credential::Login(login) => login_key(login, now)?,
    };
    let headers: Vec<(&str, &str)> = org
        .as_deref()
        .map(|org| ("x-opencode-org-id", org))
        .into_iter()
        .collect();
    let body = http.get(USAGE_URL, Auth::Bearer(&key), &headers).await?;
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

fn login_key(login: &str, now: i64) -> Result<(String, Option<String>), ProviderError> {
    let value: Value = serde_json::from_str(login)
        .map_err(|_| ProviderError::Decode("the OpenCode credential is not valid JSON"))?;
    let key = value
        .get("access")
        .and_then(json_text)
        .map(str::to_owned)
        .ok_or(ProviderError::Decode(
            "the OpenCode credential has no access token",
        ))?;
    let expires = value
        .get("expires")
        .and_then(json_number)
        .filter(|at| at.is_finite())
        .ok_or(ProviderError::Decode(
            "the OpenCode credential has no usable expiry",
        ))?;
    if expires <= 1000.0 * now as f64 {
        return Err(ProviderError::Unauthorized);
    }
    let org = value
        .pointer("/metadata/orgID")
        .and_then(json_text)
        .map(str::to_owned);
    Ok((key, org))
}
