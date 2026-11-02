// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Bucket, Http, ProviderError, epoch_seconds, json_integer, json_number,
};
use serde_json::Value;

const QUOTA_URL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";

pub async fn fetch(http: &Http, token: &str) -> Result<Vec<Bucket>, ProviderError> {
    let body = http.get(QUOTA_URL, token, &[]).await?;
    if body.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(ProviderError::Decode("the key has no coding plan"));
    }
    if body
        .get("code")
        .and_then(Value::as_i64)
        .is_some_and(|code| code != 200)
    {
        return Err(ProviderError::Decode("the quota request was rejected"));
    }
    Ok(body
        .pointer("/data/limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(window)
        .collect())
}

fn window(limit: &Value) -> Option<Bucket> {
    let kind = limit.get("type").and_then(Value::as_str)?;
    if !matches!(kind, "TOKENS_LIMIT" | "CREDIT_LIMIT") {
        return None;
    }
    let seconds = window_seconds(limit)?;
    Some(
        Bucket::timed(None, Some(seconds), None, "Z.AI")
            .used(limit.get("percentage").and_then(json_number))
            .resets(
                limit
                    .get("nextResetTime")
                    .and_then(json_number)
                    .and_then(epoch_seconds),
            ),
    )
}

fn window_seconds(limit: &Value) -> Option<i64> {
    let number = limit
        .get("number")
        .and_then(json_integer)
        .filter(|n| *n > 0)?;
    let unit = match limit.get("unit").and_then(json_integer)? {
        5 => 60,
        3 => 3_600,
        1 => 86_400,
        6 => 604_800,
        _ => return None,
    };
    number.checked_mul(unit)
}
