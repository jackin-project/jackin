// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Bucket, BucketKind, Money, ProviderError, Remaining, json_number, json_text, minor_units,
};
use serde_json::Value;

const LABEL: &str = "Extra usage";

pub(crate) fn bucket(body: &Value) -> Result<Option<Bucket>, ProviderError> {
    let spend = body.get("spend").filter(|value| value.is_object());
    if let Some(spend) = spend
        && let Some(used) = money(spend.get("used"))?
    {
        if spend.get("enabled").and_then(Value::as_bool) == Some(false) {
            return Ok(None);
        }
        let left = left(money(spend.get("limit"))?, &used)?;
        return Ok(Some(build(
            spend.get("percent").and_then(json_number),
            left,
        )));
    }
    let Some(extra) = body.get("extra_usage").filter(|value| value.is_object()) else {
        return Ok(None);
    };
    if extra.get("is_enabled").and_then(Value::as_bool) == Some(false) {
        return Ok(None);
    }
    let exponent = exponent(extra.get("decimal_places"))?;
    let unit = extra
        .get("currency")
        .and_then(json_text)
        .unwrap_or("credits");
    let minor = |key: &str| -> Result<Option<Money>, ProviderError> {
        let Some(value) = extra.get(key).and_then(Value::as_f64) else {
            return Ok(None);
        };
        let amount = minor_units(value.round()).ok_or(ProviderError::OUT_OF_RANGE)?;
        Money::new(amount, exponent, unit)
            .map(Some)
            .ok_or(ProviderError::OUT_OF_RANGE)
    };
    let Some(used) = minor("used_credits")? else {
        return Ok(None);
    };
    let left = left(minor("monthly_limit")?, &used)?;
    Ok(Some(build(
        extra.get("utilization").and_then(json_number),
        left,
    )))
}

fn left(limit: Option<Money>, used: &Money) -> Result<Option<Money>, ProviderError> {
    limit
        .map(|limit| {
            limit
                .minus(used)
                .map(|left| left.clamped_at_zero())
                .ok_or(ProviderError::OUT_OF_RANGE)
        })
        .transpose()
}

fn exponent(value: Option<&Value>) -> Result<u32, ProviderError> {
    match value {
        None | Some(Value::Null) => Ok(2),
        Some(value) => value
            .as_u64()
            .and_then(|places| u32::try_from(places).ok())
            .ok_or(ProviderError::OUT_OF_RANGE),
    }
}

fn money(value: Option<&Value>) -> Result<Option<Money>, ProviderError> {
    let Some(value) = value.filter(|value| value.is_object()) else {
        return Ok(None);
    };
    let Some(minor) = value.get("amount_minor").and_then(Value::as_i64) else {
        return Ok(None);
    };
    let unit = value
        .get("currency")
        .and_then(json_text)
        .unwrap_or("credits");
    Money::new(minor, exponent(value.get("exponent"))?, unit)
        .map(Some)
        .ok_or(ProviderError::OUT_OF_RANGE)
}

fn build(used: Option<f64>, left: Option<Money>) -> Bucket {
    Bucket::named(BucketKind::Spend, LABEL)
        .used(used)
        .left(left.map(Remaining::Amount))
}
