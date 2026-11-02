// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Bucket, BucketKind, Http, ProviderError, epoch_seconds, json_integer, json_number,
};
use serde_json::Value;

pub const PAY_AS_YOU_GO_REASON: &str = "MiniMax pay-as-you-go keys have no plan quota";

const REMAINS_URL: &str = "https://api.minimax.io/v1/token_plan/remains";
const PAY_AS_YOU_GO_PREFIX: &str = "sk-api-";
const GENERAL: &str = "general";
const EXHAUSTED: i64 = 2;
const MILLISECONDS: i64 = 1_000_000;

pub async fn fetch(http: &Http, token: &str, now: i64) -> Result<Vec<Bucket>, ProviderError> {
    if token.starts_with(PAY_AS_YOU_GO_PREFIX) {
        return Err(ProviderError::Unsupported(PAY_AS_YOU_GO_REASON));
    }
    let body = http.get(REMAINS_URL, token, &[]).await?;
    if body
        .pointer("/base_resp/status_code")
        .and_then(Value::as_i64)
        .is_some_and(|code| code != 0)
    {
        return Err(ProviderError::Decode("the provider rejected the request"));
    }
    let mut out = Vec::new();
    for model in body
        .get("model_remains")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        out.extend(model_windows(model, now));
    }
    Ok(out)
}

fn model_windows(model: &Value, now: i64) -> Vec<Bucket> {
    let name = model
        .get("model_name")
        .and_then(Value::as_str)
        .unwrap_or("model");
    let general = name == GENERAL;
    let (kind, label) = if general {
        (BucketKind::FiveHour, "5-hour")
    } else {
        (BucketKind::Other, name)
    };
    let mut out = vec![
        Bucket::named(kind, label)
            .used(used(model, "interval"))
            .resets(interval_reset(model, now)),
    ];
    if general {
        out.push(
            Bucket::named(BucketKind::Weekly, "weekly")
                .used(used(model, "weekly"))
                .resets(
                    model
                        .get("weekly_end_time")
                        .and_then(json_number)
                        .and_then(epoch_seconds),
                ),
        );
    }
    out
}

fn used(model: &Value, period: &str) -> Option<f64> {
    let remaining = model
        .get(format!("current_{period}_remaining_percent"))
        .and_then(json_number);
    if let Some(remaining) = remaining {
        return Some(100.0 - remaining);
    }
    let status = model
        .get(format!("current_{period}_status"))
        .and_then(json_integer);
    (status == Some(EXHAUSTED)).then_some(100.0)
}

fn interval_reset(model: &Value, now: i64) -> Option<i64> {
    if let Some(end) = model
        .get("end_time")
        .and_then(json_number)
        .and_then(epoch_seconds)
    {
        return Some(end);
    }
    let remains = model.get("remains_time").and_then(json_integer)?;
    let seconds = if remains > MILLISECONDS {
        remains / 1000
    } else {
        remains
    };
    Some(now.saturating_add(seconds))
}
