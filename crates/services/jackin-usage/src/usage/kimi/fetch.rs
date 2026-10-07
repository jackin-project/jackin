// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Kimi` API usage fetch and endpoint resolution.

use jackin_usage_provider_core::{
    env_value, normalize_url_or_host, provider_http_client, provider_request,
};

use super::KimiUsageResponse;

pub(crate) fn fetch_kimi_usage(token: &str) -> Result<KimiUsageResponse, String> {
    let url = resolve_kimi_usages_url();
    provider_request(
        jackin_telemetry::schema::enums::ProviderName::Kimi,
        "GET",
        "/coding/v1/usages",
        || {
            let client = provider_http_client()?;
            let response = client
                .get(&url)
                .bearer_auth(token)
                .header(reqwest::header::ACCEPT, "application/json")
                .header(reqwest::header::USER_AGENT, "jackin-capsule/usage")
                .send()
                .map_err(|err| format!("Kimi usage request failed: {err}"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(match status.as_u16() {
                    401 => "Kimi usage rejected: invalid key (HTTP 401)".to_owned(),
                    403 => "Kimi usage denied (HTTP 403)".to_owned(),
                    404 => "Kimi usage endpoint unavailable (HTTP 404)".to_owned(),
                    _ => format!("Kimi usage HTTP {status}"),
                });
            }
            response
                .json::<KimiUsageResponse>()
                .map_err(|err| format!("Kimi usage decode failed: {err}"))
        },
    )
}

/// Code API usages URL honoring the `KIMI_CODE_BASE_URL` override. Any base
/// carrying a `.../coding[/v1]` path resolves to `.../coding/v1/usages`.
pub(crate) fn resolve_kimi_usages_url() -> String {
    kimi_usages_url_from_base(env_value("KIMI_CODE_BASE_URL").as_deref())
}

pub(crate) fn kimi_usages_url_from_base(base: Option<&str>) -> String {
    const DEFAULT: &str = "https://api.kimi.com/coding/v1/usages";
    let Some(base) = base.map(str::trim).filter(|value| !value.is_empty()) else {
        return DEFAULT.to_owned();
    };
    let normalized = normalize_url_or_host(base, "");
    let trimmed = normalized.trim_end_matches('/');
    if trimmed.ends_with("/coding/v1/usages") {
        return trimmed.to_owned();
    }
    if let Some(host) = trimmed.strip_suffix("/coding/v1") {
        return format!("{host}/coding/v1/usages");
    }
    if let Some(host) = trimmed.strip_suffix("/coding") {
        return format!("{host}/coding/v1/usages");
    }
    format!("{trimmed}/coding/v1/usages")
}
