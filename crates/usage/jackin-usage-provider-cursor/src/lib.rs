// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{
    Bucket, Credential, Http, ProviderError, epoch_seconds, json_number, json_text,
};
use serde_json::{Value, json};

const PERIOD_URL: &str =
    "https://api2.cursor.sh/aiserver.v1.DashboardService/GetCurrentPeriodUsage";

pub async fn fetch(http: &Http, credential: Credential<'_>) -> Result<Vec<Bucket>, ProviderError> {
    let token = match credential {
        Credential::Token(token) => token.to_owned(),
        Credential::Login(login) => login_token(login)?,
    };
    let body = http
        .post(
            PERIOD_URL,
            &token,
            &[("connect-protocol-version", "1")],
            &json!({}),
        )
        .await?;
    let plan = body
        .get("planUsage")
        .filter(|plan| plan.is_object())
        .ok_or(ProviderError::Decode("the response has no plan usage"))?;
    let start = body.get("billingCycleStart").and_then(seconds);
    let end = body.get("billingCycleEnd").and_then(seconds);
    let length = start.zip(end).map(|(start, end)| end - start);
    Ok([
        ("totalPercentUsed", None),
        ("autoPercentUsed", Some("Auto")),
        ("apiPercentUsed", Some("API")),
    ]
    .into_iter()
    .filter_map(|(key, prefix)| {
        let used = plan.get(key).and_then(json_number)?;
        Some(
            Bucket::timed(prefix, length, None, "Plan usage")
                .used(Some(used))
                .resets(end),
        )
    })
    .collect())
}

fn seconds(value: &Value) -> Option<i64> {
    json_number(value).and_then(epoch_seconds)
}

fn login_token(login: &str) -> Result<String, ProviderError> {
    let value: Value = serde_json::from_str(login)
        .map_err(|_| ProviderError::Decode("the Cursor login file is not valid JSON"))?;
    ["accessToken", "access_token"]
        .into_iter()
        .find_map(|key| value.get(key).and_then(json_text))
        .map(str::to_owned)
        .ok_or(ProviderError::Decode(
            "the Cursor login file has no access token",
        ))
}
