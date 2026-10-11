// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Amp` API usage fetch and key loading.

use super::{AmpUsage, parse_amp_usage_output};
use jackin_usage_provider_core::{
    PROVIDER_CLI_TIMEOUT, provider_http_client, provider_request, read_json_file,
    run_cli_with_timeout,
};
use std::path::Path;

pub fn fetch_amp_api_usage(token: &str) -> Result<AmpUsage, String> {
    provider_request(
        jackin_telemetry::schema::enums::ProviderName::Amp,
        "POST",
        "/api/internal",
        || {
            let client = provider_http_client()?;
            let response = client
                .post("https://ampcode.com/api/internal?userDisplayBalanceInfo")
                .bearer_auth(token)
                .header(reqwest::header::ACCEPT, "application/json")
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .json(&serde_json::json!({
                    "method": "userDisplayBalanceInfo",
                    "params": {}
                }))
                .send()
                .map_err(|err| format!("Amp usage request failed: {err}"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!("Amp usage HTTP {status}"));
            }
            let value = response
                .json::<serde_json::Value>()
                .map_err(|err| format!("Amp usage decode failed: {err}"))?;
            AmpUsage::from_api_value(value)
                .ok_or_else(|| "Amp usage response did not include balance info".to_owned())
        },
    )
}

pub fn load_amp_api_key(path: &Path) -> Option<String> {
    let value = read_json_file(path)?;
    value
        .as_object()?
        .iter()
        .find_map(|(key, value)| {
            key.starts_with("apiKey@")
                .then(|| value.as_str())
                .flatten()
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            value
                .as_object()?
                .values()
                .filter_map(|value| value.as_str().map(str::trim))
                .find(|token| !token.is_empty())
                .map(ToOwned::to_owned)
        })
}

pub fn fetch_amp_cli_usage() -> Result<AmpUsage, String> {
    let output = run_cli_with_timeout("amp", &["--no-color", "usage"], PROVIDER_CLI_TIMEOUT)?;
    parse_amp_usage_output(&output)
        .ok_or_else(|| "Amp CLI usage output was not recognized".to_owned())
}
