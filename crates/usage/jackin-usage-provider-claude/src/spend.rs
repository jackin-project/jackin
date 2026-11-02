// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{Bucket, BucketKind, Money, Remaining, json_number, json_text};
use serde_json::Value;

const LABEL: &str = "Extra usage";

pub(crate) fn bucket(body: &Value) -> Option<Bucket> {
    let spend = body.get("spend").filter(|value| value.is_object());
    if let Some(used) = spend.and_then(|spend| money(spend.get("used"))) {
        let spend = spend?;
        if spend.get("enabled").and_then(Value::as_bool) == Some(false) {
            return None;
        }
        let left = money(spend.get("limit")).map(|limit| limit.minus(&used).clamped_at_zero());
        return Some(build(spend.get("percent").and_then(json_number), left));
    }
    let extra = body.get("extra_usage").filter(|value| value.is_object())?;
    if extra.get("is_enabled").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let exponent = extra
        .get("decimal_places")
        .and_then(Value::as_u64)
        .and_then(|places| u32::try_from(places).ok())
        .unwrap_or(2);
    let unit = extra
        .get("currency")
        .and_then(json_text)
        .unwrap_or("credits");
    let minor = |key: &str| -> Option<Money> {
        let value = extra.get(key).and_then(Value::as_f64)?;
        Some(Money::new(value.round() as i64, exponent, unit))
    };
    let used = minor("used_credits")?;
    let left = minor("monthly_limit").map(|limit| limit.minus(&used).clamped_at_zero());
    Some(build(extra.get("utilization").and_then(json_number), left))
}

fn money(value: Option<&Value>) -> Option<Money> {
    let value = value.filter(|value| value.is_object())?;
    let minor = value.get("amount_minor").and_then(Value::as_i64)?;
    let exponent = value
        .get("exponent")
        .and_then(Value::as_u64)
        .and_then(|exponent| u32::try_from(exponent).ok())
        .unwrap_or(2);
    let unit = value
        .get("currency")
        .and_then(json_text)
        .unwrap_or("credits");
    Some(Money::new(minor, exponent, unit))
}

fn build(used: Option<f64>, left: Option<Money>) -> Bucket {
    Bucket::named(BucketKind::Spend, LABEL)
        .used(used)
        .left(left.map(Remaining::Amount))
}
