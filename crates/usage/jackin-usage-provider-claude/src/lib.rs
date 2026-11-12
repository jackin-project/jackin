// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod spend;
mod windows;

use jackin_usage_provider_core::{
    Auth, Bucket, BucketKind, Credential, Http, Money, ProviderError, Remaining, json_epoch,
    json_integer, json_text, parse_iso_epoch,
};
use serde_json::Value;

pub const API_KEY_REASON: &str = "Anthropic API keys have no subscription quota";

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage?cedar_ember=1";
const API_KEY_PREFIX: &str = "sk-ant-api";

pub async fn fetch(
    http: &Http,
    credential: Credential<'_>,
    now: i64,
) -> Result<Vec<Bucket>, ProviderError> {
    let token = match credential {
        Credential::Token(token) => token.to_owned(),
        Credential::Login(login) => login_token(login, now)?,
    };
    if token.starts_with(API_KEY_PREFIX) {
        return Err(ProviderError::Unsupported(API_KEY_REASON));
    }
    let body = http
        .get(
            USAGE_URL,
            Auth::Bearer(&token),
            &[("anthropic-beta", "oauth-2025-04-20")],
        )
        .await?;
    let mut buckets = windows::buckets(&body);
    buckets.extend(spend::bucket(&body)?);
    buckets.extend(grants(&body, now)?);
    Ok(buckets)
}

fn login_token(login: &str, now: i64) -> Result<String, ProviderError> {
    let value: Value = serde_json::from_str(login)
        .map_err(|_| ProviderError::Decode("the Claude login file is not valid JSON"))?;
    let oauth = value.get("claudeAiOauth").ok_or(ProviderError::Decode(
        "the Claude login file has no claudeAiOauth entry",
    ))?;
    match oauth.get("expiresAt").and_then(json_epoch) {
        Some(at) if at > now => {}
        _ => return Err(ProviderError::Unauthorized),
    }
    ["accessToken", "access_token"]
        .into_iter()
        .find_map(|key| oauth.get(key).and_then(json_text))
        .map(str::to_owned)
        .ok_or(ProviderError::Decode(
            "the Claude login file has no access token",
        ))
}

fn grants(body: &Value, now: i64) -> Result<Vec<Bucket>, ProviderError> {
    let Some(cedar) = body.get("cedar_ember").filter(|value| value.is_object()) else {
        return Ok(Vec::new());
    };
    if cedar.get("eligible").and_then(Value::as_bool) != Some(true) {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for grant in cedar
        .get("grants")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(bucket) = grant_bucket(grant, now)? {
            out.push(bucket);
        }
    }
    Ok(out)
}

fn grant_bucket(grant: &Value, now: i64) -> Result<Option<Bucket>, ProviderError> {
    if grant.get("paused").and_then(Value::as_bool) != Some(false) {
        return Ok(None);
    }
    let Some(left) = grant
        .get("resets_left")
        .and_then(json_integer)
        .filter(|left| *left > 0)
    else {
        return Ok(None);
    };
    let ends_at = match grant.get("ends_at").filter(|value| !value.is_null()) {
        Some(value) => match value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            Some(text) => match parse_iso_epoch(text) {
                Some(at) if at > now => Some(at),
                _ => return Ok(None),
            },
            None => return Ok(None),
        },
        None => None,
    };
    let resets = Money::new(left, 0, "resets").ok_or(ProviderError::OUT_OF_RANGE)?;
    Ok(Some(
        Bucket::named(BucketKind::Other, format!("{} resets", scope(grant)))
            .left(Some(Remaining::Amount(resets)))
            .resets(ends_at),
    ))
}

fn scope(grant: &Value) -> &'static str {
    if grant.get("gives_back_weekly").and_then(Value::as_bool) == Some(true) {
        return "fable";
    }
    match grant.get("type").and_then(json_text) {
        Some("session") => "session",
        Some("weekly") => "weekly",
        Some("full") => "full",
        Some(_) | None => match clears(grant) {
            (false, false) => "unscoped",
            (true, false) => "session",
            (false, true) => "weekly",
            (true, true) => "full",
        },
    }
}

fn clears(grant: &Value) -> (bool, bool) {
    let mut five_hour = false;
    let mut weekly = false;
    for entry in grant
        .get("clears")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(json_text)
    {
        if entry == "five_hour" {
            five_hour = true;
        } else {
            weekly = true;
        }
    }
    (five_hour, weekly)
}
