// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Bucket, BucketKind, Http, Money, ProviderError, Remaining, json_money,
};
use serde_json::Value;

const KEY_URL: &str = "https://openrouter.ai/api/v1/key";
const CREDITS_URL: &str = "https://openrouter.ai/api/v1/credits";
const UNIT: &str = "USD";

pub async fn fetch(http: &Http, key: &str) -> Result<Vec<Bucket>, ProviderError> {
    let mut out = Vec::new();
    let key_body = http.get(KEY_URL, key, &[]).await?;
    out.extend(key_limit(&key_body)?);
    match http.get(CREDITS_URL, key, &[]).await {
        Ok(body) => out.extend(account_credits(&body)?),
        Err(ProviderError::Forbidden) => {}
        Err(other) => return Err(other),
    }
    Ok(out)
}

fn money(value: Option<&Value>) -> Result<Option<Money>, ProviderError> {
    match value {
        Some(value) => json_money(value, 2, UNIT),
        None => Ok(None),
    }
}

fn percent(part: &Money, whole: &Money) -> Option<f64> {
    (whole.minor() > 0).then(|| part.minor() as f64 / whole.minor() as f64 * 100.0)
}

fn key_limit(body: &Value) -> Result<Option<Bucket>, ProviderError> {
    let data = body
        .get("data")
        .filter(|data| data.is_object())
        .ok_or(ProviderError::Decode("the key response has no data object"))?;
    let Some(limit) = money(data.get("limit"))?.filter(|limit| limit.minor() > 0) else {
        return Ok(None);
    };
    let left = match money(data.get("limit_remaining"))? {
        Some(left) => Some(left),
        None => money(data.get("usage"))?
            .map(|used| limit.minus(&used).ok_or(ProviderError::OUT_OF_RANGE))
            .transpose()?,
    };
    let Some(left) = left.map(|left| left.clamped_at_zero()) else {
        return Ok(None);
    };
    let spent = limit.minus(&left).ok_or(ProviderError::OUT_OF_RANGE)?;
    Ok(Some(
        Bucket::named(BucketKind::Spend, "Key limit")
            .used(percent(&spent, &limit))
            .left(Some(Remaining::Amount(left))),
    ))
}

fn account_credits(body: &Value) -> Result<Option<Bucket>, ProviderError> {
    let data = body
        .get("data")
        .filter(|data| data.is_object())
        .ok_or(ProviderError::Decode(
            "the credits response has no data object",
        ))?;
    let total = money(data.get("total_credits"))?
        .filter(|total| total.minor() >= 0)
        .ok_or(ProviderError::Decode("the credits are not valid"))?;
    let spent = money(data.get("total_usage"))?
        .filter(|spent| spent.minor() >= 0)
        .ok_or(ProviderError::Decode("the credit usage is not valid"))?;
    let left = total
        .minus(&spent)
        .ok_or(ProviderError::OUT_OF_RANGE)?
        .clamped_at_zero();
    let used = percent(&spent, &total).map(|used| used.min(100.0));
    Ok(Some(
        Bucket::named(BucketKind::Credits, "Account credits")
            .used(used)
            .left(Some(Remaining::Amount(left))),
    ))
}
