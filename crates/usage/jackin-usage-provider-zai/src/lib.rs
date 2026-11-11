// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Auth, Bucket, Http, ProviderError, epoch_seconds, json_integer, json_number,
};
use serde_json::Value;

const QUOTA_URL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";

pub async fn fetch(http: &Http, token: &str) -> Result<Vec<Bucket>, ProviderError> {
    let body = http.get(QUOTA_URL, Auth::Raw(token), &[]).await?;
    if body.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(ProviderError::Decode("the quota response is not a success"));
    }
    if body.get("code").and_then(Value::as_i64) != Some(200) {
        return Err(ProviderError::Decode("the quota request was rejected"));
    }
    let mut out = Vec::new();
    for limit in body
        .pointer("/data/limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        out.extend(window(limit)?);
    }
    Ok(out)
}

fn window(limit: &Value) -> Result<Option<Bucket>, ProviderError> {
    let Some(kind) = limit.get("type").and_then(Value::as_str) else {
        return Ok(None);
    };
    if !matches!(kind, "TOKENS_LIMIT" | "CREDIT_LIMIT" | "TIME_LIMIT") {
        return Ok(None);
    }
    let Some(seconds) = window_seconds(limit)? else {
        return Ok(None);
    };
    Ok(Some(
        Bucket::timed(None, Some(seconds), None, "Z.AI")
            .used(limit.get("percentage").and_then(json_number))
            .resets(
                limit
                    .get("nextResetTime")
                    .and_then(json_number)
                    .and_then(epoch_seconds),
            ),
    ))
}

fn window_seconds(limit: &Value) -> Result<Option<i64>, ProviderError> {
    let Some(number) = limit
        .get("number")
        .and_then(json_integer)
        .filter(|n| *n > 0)
    else {
        return Ok(None);
    };
    let unit = match limit.get("unit").and_then(json_integer) {
        Some(3) => 3_600,
        Some(4) => 86_400,
        Some(5) => 2_592_000,
        Some(6) => 604_800,
        _ => return Ok(None),
    };
    number
        .checked_mul(unit)
        .map(Some)
        .ok_or(ProviderError::OUT_OF_RANGE)
}
