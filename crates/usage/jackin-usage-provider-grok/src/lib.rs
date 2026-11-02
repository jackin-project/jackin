// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Bucket, BucketKind, Credential, Http, Money, ProviderError, Remaining, json_number, json_text,
    parse_iso_epoch,
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
        .get(BILLING_URL, &token, &[("X-XAI-Token-Auth", "xai-grok-cli")])
        .await?;
    let config = body
        .get("config")
        .filter(|config| config.is_object())
        .ok_or(ProviderError::Decode("the billing response has no config"))?;
    let on_demand = body.get("on_demand_enabled").and_then(Value::as_bool) != Some(false);
    let mut out = Vec::new();
    out.extend(period_window(config));
    out.extend(prepaid(config));
    if on_demand {
        out.extend(on_demand_window(config));
    }
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
    let expired = entry
        .get("expires_at")
        .and_then(json_text)
        .and_then(parse_iso_epoch)
        .is_some_and(|at| at <= now);
    if expired {
        return Err(ProviderError::Unauthorized);
    }
    Ok(key.to_owned())
}

fn cents(value: Option<&Value>) -> Option<Money> {
    let minor = value?.get("val").and_then(Value::as_i64).unwrap_or(0);
    Some(Money::new(minor.abs(), 2, UNIT))
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

fn period_window(config: &Value) -> Option<Bucket> {
    let (start, end) = period(config);
    let length = start.zip(end).map(|(start, end)| end - start);
    let limit = cents(config.get("monthlyLimit")).filter(|limit| limit.minor() > 0);
    let spent = cents(config.get("used"));
    let (used, left) = if let Some(percent) = config.get("creditUsagePercent").and_then(json_number)
    {
        (percent, None)
    } else {
        let (limit, spent) = limit.zip(spent)?;
        let used = spent.minor() as f64 / limit.minor() as f64 * 100.0;
        (
            used,
            Some(Remaining::Amount(limit.minus(&spent).clamped_at_zero())),
        )
    };
    Some(
        Bucket::timed(None, length, None, "Credits")
            .used(Some(used))
            .left(left)
            .resets(end),
    )
}

fn prepaid(config: &Value) -> Option<Bucket> {
    let balance = cents(config.get("prepaidBalance")).filter(|balance| balance.minor() > 0)?;
    Some(
        Bucket::named(BucketKind::Credits, "Extra usage credits")
            .left(Some(Remaining::Amount(balance))),
    )
}

fn on_demand_window(config: &Value) -> Option<Bucket> {
    let cap = cents(config.get("onDemandCap")).filter(|cap| cap.minor() > 0)?;
    let spent = cents(config.get("onDemandUsed")).unwrap_or_else(|| Money::new(0, 2, UNIT));
    let used = spent.minor() as f64 / cap.minor() as f64 * 100.0;
    Some(
        Bucket::named(BucketKind::Spend, "On-demand")
            .used(Some(used))
            .left(Some(Remaining::Amount(cap.minus(&spent).clamped_at_zero()))),
    )
}
