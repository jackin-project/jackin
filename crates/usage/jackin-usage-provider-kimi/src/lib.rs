// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Auth, Bucket, BucketKind, Credential, Http, ProviderError, json_epoch, json_number, json_text,
};
use serde_json::Value;

pub const MAINLAND_CODING_BASE: &str = "https://api.kimi.com/coding/v1";
pub const GLOBAL_CODING_BASE: &str = "https://api.kimi.ai/coding/v1";

pub async fn fetch(
    http: &Http,
    credential: Credential<'_>,
    base: &str,
    now: i64,
) -> Result<Vec<Bucket>, ProviderError> {
    let token = match credential {
        Credential::Token(token) => token.to_owned(),
        Credential::Login(login) => login_token(login, now)?,
    };
    let body = http
        .get(&format!("{base}/usages"), Auth::Bearer(&token), &[])
        .await?;
    let usages = body.get("usages").filter(|value| value.is_object());
    let mut out = Vec::new();
    for (key, kind, label) in [
        ("limit_5h", BucketKind::FiveHour, "5-hour"),
        ("limit_7d", BucketKind::Weekly, "weekly"),
        ("limit_month_total", BucketKind::Monthly, "monthly"),
        ("limit_month_code", BucketKind::Monthly, "monthly code"),
    ] {
        out.extend(
            usages
                .and_then(|usages| usages.get(key))
                .map_or_else(|| Ok(None), |entry| window(entry, kind, label))?,
        );
    }
    Ok(out)
}

fn window(entry: &Value, kind: BucketKind, label: &str) -> Result<Option<Bucket>, ProviderError> {
    if !entry.is_object() {
        return Ok(None);
    }
    let Some(ratio) = entry.get("used_ratio").and_then(json_number) else {
        return Err(ProviderError::OUT_OF_RANGE);
    };
    Ok(Some(
        Bucket::named(kind, label)
            .used(Some(ratio * 100.0))
            .resets(entry.get("reset_time").and_then(json_epoch)),
    ))
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
