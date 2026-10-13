// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `OpenRouter` endpoint resolution and API fetch.

use super::super::refresh::ProviderError;
use super::super::{
    ProviderHttpError, UsageSnapshotStatus, env_value, get_json_bearer, provider_http_client,
    provider_request,
};

use super::{OpenRouterCreditsOutcome, OpenRouterModelCheck, parse_openrouter_credits};

pub(crate) const OPENROUTER_DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

pub(crate) fn openrouter_base_url() -> String {
    openrouter_base_url_from(
        env_value("OPENROUTER_API_URL").as_deref(),
        env_value("OPENROUTER_BASE_URL").as_deref(),
    )
}

/// Pure base-URL resolution: `OPENROUTER_API_URL` wins over
/// `OPENROUTER_BASE_URL`, blank inputs fall through to the next candidate.
/// The hermetic seam tests use so live env can never vacate assertions.
pub(crate) fn openrouter_base_url_from(api_url: Option<&str>, base_url: Option<&str>) -> String {
    [api_url, base_url]
        .into_iter()
        .flatten()
        .map(|value| value.trim().trim_end_matches('/'))
        .find(|value| !value.is_empty())
        .map_or_else(|| OPENROUTER_DEFAULT_BASE_URL.to_owned(), str::to_owned)
}

pub(crate) fn check_openrouter_model_in_catalog(
    catalog: &serde_json::Value,
    model_id: &str,
) -> OpenRouterModelCheck {
    let found = catalog
        .get("data")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|models| {
            models.iter().any(|model| {
                model
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| id == model_id)
            })
        });
    if found {
        OpenRouterModelCheck::Verified {
            model_id: model_id.to_owned(),
        }
    } else {
        OpenRouterModelCheck::Unverified {
            model_id: model_id.to_owned(),
            reason: "not in catalog snapshot; unverified".to_owned(),
        }
    }
}

pub(crate) fn fetch_openrouter_key_usage(
    base_url: &str,
    key: &str,
) -> Result<serde_json::Value, ProviderHttpError> {
    get_json_bearer::<serde_json::Value>(
        jackin_telemetry::schema::enums::ProviderName::Openrouter,
        "GET",
        "OpenRouter key",
        &format!("{base_url}/key"),
        key,
        &[],
    )
}

pub(crate) fn openrouter_key_error_status(error: &ProviderError) -> UsageSnapshotStatus {
    match error.status() {
        Some(401) => UsageSnapshotStatus::NeedsLogin,
        _ => UsageSnapshotStatus::Error,
    }
}

/// `/credits` never fails the snapshot: every outcome (including the typed 403
/// management-scope mismatch) is a value, so `/key` rows always survive it.
pub(crate) fn fetch_openrouter_credits(base_url: &str, key: &str) -> OpenRouterCreditsOutcome {
    provider_request(
        jackin_telemetry::schema::enums::ProviderName::Openrouter,
        "GET",
        "/credits",
        || {
            let client = provider_http_client()?;
            let response = client
                .get(format!("{base_url}/credits"))
                .bearer_auth(key)
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .map_err(|error| format!("OpenRouter credits request failed: {error}"))?;
            let status = response.status();
            if !status.is_success() {
                return Ok(OpenRouterCreditsOutcome::from_http_status(status.as_u16()));
            }
            let value = response
                .json::<serde_json::Value>()
                .map_err(|error| format!("OpenRouter credits decode failed: {error}"))?;
            Ok(parse_openrouter_credits(value)
                .unwrap_or_else(OpenRouterCreditsOutcome::Unavailable))
        },
    )
    .unwrap_or_else(OpenRouterCreditsOutcome::Unavailable)
}

/// Catalog validation never errors: any fetch failure degrades to `Unverified`
/// (a stale catalog must not reject a configured model).
pub(crate) fn fetch_openrouter_model_check(base_url: &str, model_id: &str) -> OpenRouterModelCheck {
    let unverified = |reason: String| OpenRouterModelCheck::Unverified {
        model_id: model_id.to_owned(),
        reason,
    };
    let catalog = provider_request(
        jackin_telemetry::schema::enums::ProviderName::Openrouter,
        "GET",
        "/models",
        || {
            let client = provider_http_client()?;
            let response = client
                .get(format!("{base_url}/models"))
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .map_err(|error| format!("OpenRouter models request failed: {error}"))?;
            if !response.status().is_success() {
                return Err(format!("OpenRouter models HTTP {}", response.status()));
            }
            response
                .json::<serde_json::Value>()
                .map_err(|error| format!("OpenRouter models decode failed: {error}"))
        },
    );
    match catalog {
        Ok(value) => check_openrouter_model_in_catalog(&value, model_id),
        Err(error) => unverified(error),
    }
}
