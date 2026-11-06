// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Bucket, BucketKind, Credential, Http, ProviderError, json_epoch, json_integer, json_number,
    json_text,
};
use serde_json::Value;

const USAGES_URL: &str = "https://api.kimi.com/coding/v1/usages";
const RESET_KEYS: [&str; 4] = ["resetTime", "reset_at", "resetAt", "reset_time"];

pub async fn fetch(
    http: &Http,
    credential: Credential<'_>,
    now: i64,
) -> Result<Vec<Bucket>, ProviderError> {
    let token = match credential {
        Credential::Token(token) => token.to_owned(),
        Credential::Login(login) => login_token(login, now)?,
    };
    let body = http.get(USAGES_URL, &token, &[]).await?;
    let mut out = Vec::new();
    for (index, item) in body
        .get("limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        out.extend(limit_row(item, index + 1)?);
    }
    out.extend(body.get("usage").and_then(summary_row));
    Ok(out)
}

fn login_token(login: &str, now: i64) -> Result<String, ProviderError> {
    let value: Value = serde_json::from_str(login)
        .map_err(|_| ProviderError::Decode("the Kimi login file is not valid JSON"))?;
    let token = value
        .get("access_token")
        .and_then(json_text)
        .ok_or(ProviderError::Decode(
            "the Kimi login file has no access token",
        ))?;
    let expired = value
        .get("expires_at")
        .and_then(json_epoch)
        .is_some_and(|at| at <= now);
    if expired {
        return Err(ProviderError::Unauthorized);
    }
    Ok(token.to_owned())
}

struct Row {
    used: f64,
    reset: Option<i64>,
}

fn row(detail: &Value) -> Option<Row> {
    let count = |key: &str| detail.get(key).and_then(json_number);
    let limit = count("limit").filter(|limit| *limit > 0.0)?;
    let used = count("used").or_else(|| count("remaining").map(|left| limit - left))?;
    let reset = RESET_KEYS
        .iter()
        .find_map(|key| detail.get(key))
        .and_then(json_epoch);
    Some(Row {
        used: used / limit * 100.0,
        reset,
    })
}

fn limit_row(item: &Value, number: usize) -> Result<Option<Bucket>, ProviderError> {
    let detail = item
        .get("detail")
        .filter(|value| value.is_object())
        .unwrap_or(item);
    let Some(row) = row(detail) else {
        return Ok(None);
    };
    let seconds = match item.get("window") {
        Some(window) => window_seconds(window)?,
        None => None,
    };
    let name = detail
        .get("name")
        .and_then(json_text)
        .or_else(|| detail.get("title").and_then(json_text))
        .map_or_else(|| format!("Limit {number}"), str::to_owned);
    Ok(Some(
        Bucket::timed(None, seconds, None, &name)
            .used(Some(row.used))
            .resets(row.reset),
    ))
}

fn summary_row(usage: &Value) -> Option<Bucket> {
    let row = row(usage)?;
    Some(
        Bucket::timed(None, None, Some(BucketKind::Weekly), "weekly")
            .used(Some(row.used))
            .resets(row.reset),
    )
}

fn window_seconds(window: &Value) -> Result<Option<i64>, ProviderError> {
    let Some(duration) = window.get("duration").and_then(json_integer) else {
        return Ok(None);
    };
    let unit = match window.get("timeUnit").and_then(json_text) {
        Some("TIME_UNIT_SECOND") => 1,
        Some("TIME_UNIT_MINUTE") => 60,
        Some("TIME_UNIT_HOUR") => 3_600,
        Some("TIME_UNIT_DAY") => 86_400,
        Some("TIME_UNIT_WEEK") => 604_800,
        _ => return Ok(None),
    };
    duration
        .checked_mul(unit)
        .map(Some)
        .ok_or(ProviderError::OUT_OF_RANGE)
}
