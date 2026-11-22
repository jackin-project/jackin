// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{Auth, Http, ProviderError, json_text, parse_iso_epoch};
use serde_json::{Value, json};

#[derive(Debug)]
pub struct Profile {
    pub subject: String,
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug)]
pub struct CodeAssistProject {
    pub id: String,
    pub tier_id: Option<String>,
    pub tier_name: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct QuotaBucket {
    pub model: Option<String>,
    pub token_type: Option<String>,
    pub remaining_amount: Option<u64>,
    pub remaining_fraction: Option<f64>,
    pub resets_at: Option<i64>,
}

pub async fn identify(http: &Http, token: &str) -> Result<Profile, ProviderError> {
    let body = http
        .get(
            "https://www.googleapis.com/oauth2/v2/userinfo",
            Auth::Bearer(token),
            &[],
        )
        .await?;
    let text = |key| body.get(key).and_then(json_text).map(str::to_owned);
    Ok(Profile {
        subject: text("id").ok_or(ProviderError::Decode("Google returned no account ID"))?,
        email: text("email"),
        name: text("name"),
    })
}

pub async fn load_project(
    http: &Http,
    token: &str,
    project: Option<&str>,
) -> Result<CodeAssistProject, ProviderError> {
    if token.starts_with(super::API_KEY_PREFIX) {
        return Err(ProviderError::Unsupported(super::API_KEY_REASON));
    }
    let mut request = json!({"metadata":{"ideType":"IDE_UNSPECIFIED","platform":"PLATFORM_UNSPECIFIED","pluginType":"GEMINI"}});
    if let Some(id) = project {
        if id.trim().is_empty() || id.chars().all(|c| c.is_ascii_digit()) {
            return Err(ProviderError::Unsupported(
                "enter a Google Cloud project ID",
            ));
        }
        request["cloudaicompanionProject"] = json!(id);
        request["metadata"]["duetProject"] = json!(id);
    }
    let body = http
        .post(super::LOAD_URL, Auth::Bearer(token), &[], &request)
        .await?;
    let Some(current_tier) = body.get("currentTier").filter(|tier| tier.is_object()) else {
        return onboard(http, token, project, &body, &request["metadata"]).await;
    };
    let id = body
        .get("cloudaicompanionProject")
        .and_then(json_text)
        .or(project)
        .ok_or(ProviderError::Unsupported(
            "select a Code Assist project for this Google account",
        ))?;
    if project.is_some_and(|requested| requested != id) {
        return Err(ProviderError::Unsupported(
            "Google selected a different Code Assist project",
        ));
    }
    let field = |key| {
        body.get("paidTier")
            .and_then(|tier| tier.get(key))
            .and_then(json_text)
            .or_else(|| current_tier.get(key).and_then(json_text))
            .map(str::to_owned)
    };
    Ok(CodeAssistProject {
        id: id.to_owned(),
        tier_id: field("id"),
        tier_name: field("name"),
    })
}

async fn onboard(
    http: &Http,
    token: &str,
    project: Option<&str>,
    eligibility: &Value,
    metadata: &Value,
) -> Result<CodeAssistProject, ProviderError> {
    let default = eligibility["allowedTiers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|tier| tier["isDefault"] == true);
    if default.is_none()
        && (project.is_none()
            || eligibility["ineligibleTiers"]
                .as_array()
                .is_some_and(|v| !v.is_empty()))
    {
        return Err(ProviderError::Unsupported(
            "Google requires an eligible Code Assist tier or a Cloud project",
        ));
    }
    let tier = default
        .and_then(|t| t["id"].as_str())
        .unwrap_or("legacy-tier");
    let mut request = json!({"tierId":tier,"metadata":metadata});
    if tier == "free-tier" {
        if let Some(meta) = request["metadata"].as_object_mut() {
            meta.remove("duetProject");
        }
    } else {
        let id = project.ok_or(ProviderError::Unsupported(
            "select a Cloud project for the Google Code Assist tier",
        ))?;
        request["cloudaicompanionProject"] = json!(id);
        request["metadata"]["duetProject"] = json!(id);
    }
    let mut operation = http
        .post(
            "https://cloudcode-pa.googleapis.com/v1internal:onboardUser",
            Auth::Bearer(token),
            &[],
            &request,
        )
        .await?;
    let mut elapsed = 0;
    while operation["done"] != true && operation["error"].is_null() {
        let name = operation["name"]
            .as_str()
            .and_then(|s| s.strip_prefix("operations/"))
            .filter(|s| {
                !s.is_empty()
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
                    && *s != "."
                    && *s != ".."
            })
            .ok_or(ProviderError::Decode("invalid Google onboarding operation"))?;
        if elapsed >= 120 {
            return Err(ProviderError::Network(
                "Google Code Assist enrollment is pending; retry sign-in",
            ));
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        elapsed += 5;
        operation = http
            .get(
                &format!("https://cloudcode-pa.googleapis.com/v1internal/operations/{name}"),
                Auth::Bearer(token),
                &[],
            )
            .await?;
    }
    if !operation["error"].is_null() {
        return Err(ProviderError::Unsupported(
            "Google declined Code Assist enrollment",
        ));
    }
    let managed = operation
        .pointer("/response/cloudaicompanionProject/id")
        .and_then(json_text);
    let id =
        managed
            .or(project.filter(|_| tier != "free-tier"))
            .ok_or(ProviderError::Unsupported(
                "Google did not return an enrolled Code Assist project",
            ))?;
    if project.is_some_and(|p| p != id) {
        return Err(ProviderError::Unsupported(
            "Google selected a different Code Assist project",
        ));
    }
    Ok(CodeAssistProject {
        id: id.to_owned(),
        tier_id: Some(tier.to_owned()),
        tier_name: default.and_then(|t| t["name"].as_str()).map(str::to_owned),
    })
}

pub async fn fetch_buckets(
    http: &Http,
    token: &str,
    project: &str,
) -> Result<Vec<QuotaBucket>, ProviderError> {
    let body = http
        .post(
            super::QUOTA_URL,
            Auth::Bearer(token),
            &[],
            &json!({"project":project}),
        )
        .await?;
    body.get("buckets")
        .and_then(Value::as_array)
        .ok_or(ProviderError::Decode(
            "Code Assist returned no quota inventory",
        ))?
        .iter()
        .map(quota_bucket)
        .collect()
}

fn quota_bucket(entry: &Value) -> Result<QuotaBucket, ProviderError> {
    if !entry.is_object() {
        return Err(ProviderError::Decode("invalid Code Assist quota bucket"));
    }
    let field = |key| entry.get(key).and_then(json_text);
    let amount = field("remainingAmount")
        .map(str::parse::<u64>)
        .transpose()
        .map_err(|_| ProviderError::OUT_OF_RANGE)?;
    let fraction = entry.get("remainingFraction").and_then(Value::as_f64);
    if fraction.is_some_and(|n| !(0.0..=1.0).contains(&n)) {
        return Err(ProviderError::OUT_OF_RANGE);
    }
    Ok(QuotaBucket {
        model: field("modelId").map(str::to_owned),
        token_type: field("tokenType").map(str::to_owned),
        remaining_amount: amount,
        remaining_fraction: fraction,
        resets_at: field("resetTime")
            .map(|s| {
                parse_iso_epoch(s).ok_or(ProviderError::Decode("invalid Code Assist reset time"))
            })
            .transpose()?,
    })
}
