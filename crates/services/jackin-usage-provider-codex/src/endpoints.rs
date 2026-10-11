// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Codex` endpoint resolution.

use jackin_telemetry::ResultTelemetryExt as _;
use jackin_usage_provider_core::parse_chatgpt_base_url;
use std::fs;
use std::path::Path;

pub fn resolve_codex_usage_url(codex_home: &Path) -> String {
    let normalized = resolve_codex_base_url(codex_home);
    let path = if normalized.contains("/backend-api") {
        "/wham/usage"
    } else {
        "/api/codex/usage"
    };
    format!("{normalized}{path}")
}

pub fn resolve_codex_reset_credits_url(codex_home: &Path) -> String {
    format!(
        "{}/wham/rate-limit-reset-credits",
        resolve_codex_base_url(codex_home)
    )
}

pub fn resolve_codex_base_url(codex_home: &Path) -> String {
    let config_path = codex_home.join("config.toml");
    let contents = match fs::read_to_string(&config_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        result => result
            .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError)
            .ok(),
    };
    let base = contents
        .and_then(|contents| parse_chatgpt_base_url(&contents))
        .unwrap_or_else(|| "https://chatgpt.com/backend-api".to_owned());
    let mut normalized = base.trim().trim_end_matches('/').to_owned();
    if normalized.is_empty() {
        normalized = "https://chatgpt.com/backend-api".to_owned();
    }
    if (normalized.starts_with("https://chatgpt.com")
        || normalized.starts_with("https://chat.openai.com"))
        && !normalized.contains("/backend-api")
    {
        normalized.push_str("/backend-api");
    }
    normalized
}
