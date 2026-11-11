// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Auth, Bucket, BucketKind, Credential, Http, ProviderError, Remaining, json_number, json_text,
    parse_iso_epoch,
};
use serde_json::{Value, json};

pub const API_KEY_REASON: &str = "Gemini API keys have no quota method";

const LOAD_URL: &str = "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist";
const QUOTA_URL: &str = "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";
const API_KEY_PREFIX: &str = "AIza";

pub async fn fetch(
    http: &Http,
    credential: Credential<'_>,
    now: i64,
) -> Result<Vec<Bucket>, ProviderError> {
    let token = match credential {
        Credential::Token(token) => token.to_owned(),
        Credential::Login(login) => login_token(login, now)?,
    };
    if token.starts_with(API_KEY_PREFIX) {
        return Err(ProviderError::Unsupported(API_KEY_REASON));
    }
    let load = http
        .post(
            LOAD_URL,
            Auth::Bearer(&token),
            &[],
            &json!({"metadata": {
                "ideType": "IDE_UNSPECIFIED",
                "platform": "PLATFORM_UNSPECIFIED",
                "pluginType": "GEMINI",
            }}),
        )
        .await?;
    let project = load
        .get("cloudaicompanionProject")
        .and_then(json_text)
        .ok_or(ProviderError::Decode(
            "the account has no Code Assist project",
        ))?;
    let quota = http
        .post(
            QUOTA_URL,
            Auth::Bearer(&token),
            &[],
            &json!({"project": project}),
        )
        .await?;
    Ok(quota
        .get("buckets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(bucket)
        .collect())
}

fn login_token(login: &str, now: i64) -> Result<String, ProviderError> {
    let value: Value = serde_json::from_str(login)
        .map_err(|_| ProviderError::Decode("the Gemini login file is not valid JSON"))?;
    let token = value
        .get("access_token")
        .and_then(json_text)
        .ok_or(ProviderError::Decode(
            "the Gemini login file has no access token",
        ))?;
    let expired = value
        .get("expiry_date")
        .and_then(Value::as_i64)
        .is_some_and(|at| now.checked_mul(1000).is_none_or(|now_ms| at <= now_ms));
    if expired {
        return Err(ProviderError::Unauthorized);
    }
    Ok(token.to_owned())
}

fn bucket(entry: &Value) -> Option<Bucket> {
    let model = entry.get("modelId").and_then(json_text)?;
    let fraction = entry.get("remainingFraction").and_then(json_number)?;
    let label = match entry.get("tokenType").and_then(json_text) {
        Some(kind) => format!("{model} {}", kind.to_lowercase()),
        None => model.to_owned(),
    };
    Some(
        Bucket::named(BucketKind::Other, label)
            .used(Some((1.0 - fraction) * 100.0))
            .left(Some(Remaining::Percent(fraction * 100.0)))
            .resets(
                entry
                    .get("resetTime")
                    .and_then(json_text)
                    .and_then(parse_iso_epoch),
            ),
    )
}
