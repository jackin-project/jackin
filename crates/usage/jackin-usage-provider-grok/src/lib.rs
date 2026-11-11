// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Auth, Bucket, BucketKind, Credential, Http, Money, ProviderError, Remaining, json_number,
    json_text, parse_iso_epoch,
};
use serde_json::Value;

const BILLING_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
const PREFERRED_SCOPE: &str = "https://auth.x.ai::";
const FALLBACK_SCOPE: &str = "/sign-in";
const UNIT: &str = "USD";

pub async fn fetch(
    http: &Http,
    credential: Credential<'_>,
    now: i64,
) -> Result<Vec<Bucket>, ProviderError> {
    let token = match credential {
        Credential::Token(token) => token.to_owned(),
        Credential::Login(login) => login_key(login, now)?,
    };
    let body = http
        .get(
            BILLING_URL,
            Auth::Bearer(&token),
            &[("X-XAI-Token-Auth", "xai-grok-cli")],
        )
        .await?;
    let config = body.get("config").unwrap_or(&Value::Null);
    let on_demand = body.get("on_demand_enabled").and_then(Value::as_bool) == Some(true);
    let mut out = Vec::new();
    out.extend(period_window(config)?);
    out.extend(prepaid(config)?);
    if on_demand {
        out.extend(on_demand_window(config)?);
    }
    out.extend(product_share(config)?);
    Ok(out)
}

fn login_key(login: &str, now: i64) -> Result<String, ProviderError> {
    let value: Value = serde_json::from_str(login)
        .map_err(|_| ProviderError::Decode("the Grok login file is not valid JSON"))?;
    let entries = value
        .as_object()
        .ok_or(ProviderError::Decode("the Grok login file has no entries"))?;
    let entry = entries
        .iter()
        .find(|(scope, _)| scope.starts_with(PREFERRED_SCOPE))
        .or_else(|| {
            entries
                .iter()
                .find(|(scope, _)| scope.contains(FALLBACK_SCOPE))
        })
        .map(|(_, entry)| entry)
        .ok_or(ProviderError::Decode(
            "the Grok login file has no usable entry",
        ))?;
    let key = entry
        .get("key")
        .and_then(json_text)
        .ok_or(ProviderError::Decode("the Grok login entry has no key"))?;
    match entry
        .get("expires_at")
        .and_then(json_text)
        .and_then(parse_iso_epoch)
    {
        Some(at) if at > now => {}
        _ => return Err(ProviderError::Unauthorized),
    }
    Ok(key.to_owned())
}

fn cents(value: Option<&Value>) -> Result<Option<Money>, ProviderError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let minor = match value.get("val") {
        None | Some(Value::Null) => 0,
        Some(number) => number.as_i64().ok_or(ProviderError::OUT_OF_RANGE)?,
    };
    if minor < 0 {
        return Err(ProviderError::OUT_OF_RANGE);
    }
    Ok(Some(Money::cents(minor, UNIT)))
}

fn period(config: &Value) -> (Option<i64>, Option<i64>) {
    let at = |current: &str, deprecated: &str| {
        config
            .pointer(current)
            .or_else(|| config.get(deprecated))
            .and_then(json_text)
            .and_then(parse_iso_epoch)
    };
    (
        at("/currentPeriod/start", "billingPeriodStart"),
        at("/currentPeriod/end", "billingPeriodEnd"),
    )
}

fn period_window(config: &Value) -> Result<Option<Bucket>, ProviderError> {
    let (start, end) = period(config);
    let length = match start.zip(end) {
        Some((start, end)) => Some(end.checked_sub(start).ok_or(ProviderError::OUT_OF_RANGE)?),
        None => None,
    };
    let (used, left) = if let Some(percent) = config.get("creditUsagePercent").and_then(json_number)
    {
        (percent, None)
    } else {
        let limit = cents(config.get("monthlyLimit"))?.filter(|limit| limit.minor() > 0);
        let Some((limit, spent)) = limit.zip(cents(config.get("used"))?) else {
            return Ok(None);
        };
        let used = spent.minor() as f64 / limit.minor() as f64 * 100.0;
        let left = limit.minus(&spent).ok_or(ProviderError::OUT_OF_RANGE)?;
        (used, Some(Remaining::Amount(left.clamped_at_zero())))
    };
    Ok(Some(
        Bucket::timed(None, length, None, "Credits")
            .used(Some(used))
            .left(left)
            .resets(end),
    ))
}

fn prepaid(config: &Value) -> Result<Option<Bucket>, ProviderError> {
    Ok(cents(config.get("prepaidBalance"))?
        .filter(|balance| balance.minor() > 0)
        .map(|balance| {
            Bucket::named(BucketKind::Credits, "Extra usage credits")
                .left(Some(Remaining::Amount(balance)))
        }))
}

fn on_demand_window(config: &Value) -> Result<Option<Bucket>, ProviderError> {
    let Some(cap) = cents(config.get("onDemandCap"))?.filter(|cap| cap.minor() > 0) else {
        return Ok(None);
    };
    let spent = cents(config.get("onDemandUsed"))?.unwrap_or_else(|| Money::cents(0, UNIT));
    let used = spent.minor() as f64 / cap.minor() as f64 * 100.0;
    let left = cap.minus(&spent).ok_or(ProviderError::OUT_OF_RANGE)?;
    Ok(Some(
        Bucket::named(BucketKind::Spend, "On-demand")
            .used(Some(used))
            .left(Some(Remaining::Amount(left.clamped_at_zero()))),
    ))
}

fn product_share(config: &Value) -> Result<Option<Bucket>, ProviderError> {
    let row = config
        .get("productUsage")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|row| row.get("product").and_then(json_text) == Some("GrokBuild"));
    let Some(used) = row
        .and_then(|row| row.get("usagePercent").and_then(json_number))
    else {
        return Ok(None);
    };
    Ok(Some(
        Bucket::named(BucketKind::Other, "Grok Build share").used(Some(used)),
    ))
}
