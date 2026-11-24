// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Auth, Bucket, BucketKind, Exact, Http, Money, ProviderError, Remaining,
};

const KEY_URL: &str = "https://openrouter.ai/api/v1/key";
const UNIT: &str = "USD";
const PLACES: u32 = 2;

const SPENDS: [(&str, &str); 4] = [
    ("usage", "Key spend"),
    ("usage_daily", "Daily spend"),
    ("usage_weekly", "Weekly spend"),
    ("usage_monthly", "Monthly spend"),
];

pub async fn fetch(http: &Http, key: &str) -> Result<Vec<Bucket>, ProviderError> {
    let body = http.get_exact(KEY_URL, Auth::Bearer(key), &[]).await?;
    key_windows(&body)
}

fn money(value: &Exact) -> Result<Option<Money>, ProviderError> {
    Ok(value
        .decimal()?
        .map(|amount| Money::exact(amount, PLACES, UNIT)))
}

fn key_windows(body: &Exact) -> Result<Vec<Bucket>, ProviderError> {
    let data = &body["data"];
    if !data.is_object() {
        return Err(ProviderError::Decode("the key response has no data object"));
    }
    let mut out = Vec::new();
    out.extend(key_budget(data)?);
    for (field, label) in SPENDS {
        if let Some(spend) = money(&data[field])? {
            out.push(Bucket::named(BucketKind::Spend, label).spent(Some(Remaining::Amount(spend))));
        }
    }
    Ok(out)
}

fn key_budget(data: &Exact) -> Result<Option<Bucket>, ProviderError> {
    let Some(limit) = money(&data["limit"])?.filter(Money::is_positive) else {
        return Ok(None);
    };
    let left = match money(&data["limit_remaining"])? {
        Some(left) => Some(left),
        None => money(&data["usage"])?
            .map(|used| limit.minus(&used).ok_or(ProviderError::OUT_OF_RANGE))
            .transpose()?,
    };
    let Some(left) = left.map(|left| left.clamped_at_zero()) else {
        return Ok(None);
    };
    let spent = limit.minus(&left).ok_or(ProviderError::OUT_OF_RANGE)?;
    Ok(Some(
        Bucket::named(BucketKind::Spend, "Key limit")
            .used(spent.percent_of(&limit))
            .left(Some(Remaining::Amount(left))),
    ))
}
