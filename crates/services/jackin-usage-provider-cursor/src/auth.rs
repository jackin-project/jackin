// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use base64::Engine as _;
use jackin_usage_provider_core::{env_value, home_path, read_json_file};
use std::path::PathBuf;

pub(crate) const CURSOR_DEFAULT_DASHBOARD_BASE: &str = "https://api2.cursor.sh";
pub const CURSOR_SESSION_BASE: &str = "https://cursor.com";

#[expect(
    missing_debug_implementations,
    reason = "credential type: a live access token must never be formatted into a log or error"
)]
#[derive(Clone)]
pub struct CursorAuth {
    pub(crate) access_token: String,
    pub(crate) user_id: Option<String>,
}

pub fn cursor_auth_path() -> PathBuf {
    env_value("CURSOR_CONFIG_DIR").map_or_else(
        || home_path(".cursor/auth.json"),
        |dir| PathBuf::from(dir).join("auth.json"),
    )
}

pub fn cursor_auth_from_value(value: &serde_json::Value) -> Option<CursorAuth> {
    let access_token = ["accessToken", "access_token"]
        .into_iter()
        .filter_map(|key| value.get(key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .find(|token| !token.is_empty())?
        .to_owned();
    Some(CursorAuth {
        user_id: cursor_user_id_from_token(&access_token),
        access_token,
    })
}

pub fn load_cursor_auth() -> Result<CursorAuth, String> {
    let path = cursor_auth_path();
    let value = read_json_file(&path)
        .ok_or_else(|| "Cursor auth.json is missing or unreadable".to_owned())?;
    cursor_auth_from_value(&value).ok_or_else(|| "Cursor access token is missing".to_owned())
}

pub fn cursor_user_id_from_token(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim())
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    value
        .get("sub")
        .and_then(serde_json::Value::as_str)
        .and_then(|sub| sub.split('|').next_back())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

pub fn cursor_cli_identity_from_value(value: &serde_json::Value) -> Option<String> {
    let info = value.get("authInfo")?;
    ["email", "displayName", "display_name", "userId"]
        .into_iter()
        .filter_map(|key| info.get(key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .find(|identity| !identity.is_empty())
        .map(str::to_owned)
}

pub fn load_cursor_cli_identity() -> Option<String> {
    let path = env_value("CURSOR_CONFIG_DIR").map_or_else(
        || home_path(".cursor/cli-config.json"),
        |dir| PathBuf::from(dir).join("cli-config.json"),
    );
    let value = read_json_file(&path)?;
    cursor_cli_identity_from_value(&value)
}

pub fn cursor_identity_from_cli_config(value: &serde_json::Value) -> Option<String> {
    cursor_cli_identity_from_value(value)
}

pub fn cursor_dashboard_base() -> String {
    env_value("CURSOR_API_ENDPOINT").unwrap_or_else(|| CURSOR_DEFAULT_DASHBOARD_BASE.to_owned())
}

pub fn cursor_default_base() -> bool {
    env_value("CURSOR_API_ENDPOINT").is_none()
}

pub fn cursor_dashboard_url_with_base(base: &str, method: &str) -> String {
    format!(
        "{}/aiserver.v1.DashboardService/{method}",
        base.trim_end_matches('/')
    )
}
