// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Auth, Bucket, BucketKind, Http, Money, ProviderError, Remaining, json_money,
};
use serde_json::Value;

const KEY_URL: &str = "https://openrouter.ai/api/v1/key";
const UNIT: &str = "USD";

const SPENDS: [(&str, &str); 4] = [
    ("usage", "Key spend"),
    ("usage_daily", "Daily spend"),
    ("usage_weekly", "Weekly spend"),
    ("usage_monthly", "Monthly spend"),
];

pub async fn fetch(http: &Http, key: &str) -> Result<Vec<Bucket>, ProviderError> {
    let body = http.get(KEY_URL, Auth::Bearer(key), &[]).await?;
    key_windows(&body)
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

fn key_windows(body: &Value) -> Result<Vec<Bucket>, ProviderError> {
    let data = body
        .get("data")
        .filter(|data| data.is_object())
        .ok_or(ProviderError::Decode("the key response has no data object"))?;
    let mut out = Vec::new();
    out.extend(key_budget(data)?);
    for (field, label) in SPENDS {
        if let Some(spend) = money(data.get(field))? {
            out.push(Bucket::named(BucketKind::Spend, label).spent(Some(Remaining::Amount(spend))));
        }
    }
    Ok(out)
}

fn key_budget(data: &Value) -> Result<Option<Bucket>, ProviderError> {
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
