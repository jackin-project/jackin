// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Auth, Bucket, BucketKind, Credential, Http, ProviderError, epoch_seconds, json_integer,
    json_number, json_text,
};
use serde_json::Value;

pub const PAY_AS_YOU_GO_REASON: &str = "MiniMax pay-as-you-go keys have no plan quota";

const REMAINS_URL: &str = "https://www.minimax.io/v1/token_plan/remains";
const CODING_PLAN_URL: &str =
    "https://platform.minimax.io/v1/api/openplatform/coding_plan/remains?business=Agent&device_platform=mcode";
const PAY_AS_YOU_GO_PREFIX: &str = "sk-api-";
const GENERAL: &str = "general";
const EXHAUSTED: i64 = 2;

pub async fn fetch(
    http: &Http,
    credential: Credential<'_>,
    now: i64,
) -> Result<Vec<Bucket>, ProviderError> {
    let (url, token) = match credential {
        Credential::Token(token) => {
            if token.starts_with(PAY_AS_YOU_GO_PREFIX) {
                return Err(ProviderError::Unsupported(PAY_AS_YOU_GO_REASON));
            }
            (REMAINS_URL, token.to_owned())
        }
        Credential::Login(login) => (CODING_PLAN_URL, login_token(login, now)?),
    };
    let body = http.get(url, Auth::Bearer(&token), &[]).await?;
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
        out.extend(model_windows(model, now)?);
    }
    Ok(out)
}

fn login_token(login: &str, now: i64) -> Result<String, ProviderError> {
    let value: Value = serde_json::from_str(login)
        .map_err(|_| ProviderError::Decode("the MiniMax login file is not valid JSON"))?;
    let records = value
        .get("records")
        .and_then(Value::as_object)
        .ok_or(ProviderError::Decode(
            "the MiniMax login file has no records map",
        ))?;
    let mut usable = records.values().filter(|record| {
        record
            .get("accessToken")
            .and_then(json_text)
            .is_some_and(|token| !token.is_empty())
    });
    let Some(record) = usable.next() else {
        return Err(ProviderError::Decode(
            "the MiniMax login file has no usable credential",
        ));
    };
    if usable.next().is_some() {
        return Err(ProviderError::Decode(
            "the MiniMax login file holds more than one credential",
        ));
    }
    let expires = record
        .get("expiresAtMs")
        .and_then(Value::as_f64)
        .filter(|at| at.is_finite())
        .ok_or(ProviderError::Decode(
            "the MiniMax credential has no usable expiry",
        ))?;
    if expires <= 1000.0 * now as f64 {
        return Err(ProviderError::Unauthorized);
    }
    record
        .get("accessToken")
        .and_then(json_text)
        .map(str::to_owned)
        .ok_or(ProviderError::Decode(
            "the MiniMax credential has no access token",
        ))
}

fn model_windows(model: &Value, now: i64) -> Result<Vec<Bucket>, ProviderError> {
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
            .resets(interval_reset(model, now)?),
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
    Ok(out)
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

fn interval_reset(model: &Value, now: i64) -> Result<Option<i64>, ProviderError> {
    if let Some(end) = model
        .get("end_time")
        .and_then(json_number)
        .and_then(epoch_seconds)
    {
        return Ok(Some(end));
    }
    let Some(remains) = model.get("remains_time").and_then(json_integer) else {
        return Ok(None);
    };
    now.checked_add(remains / 1000)
        .map(Some)
        .ok_or(ProviderError::OUT_OF_RANGE)
}
