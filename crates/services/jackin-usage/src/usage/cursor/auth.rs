// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Cursor` auth, identity, and dashboard endpoints.

use super::super::*;

pub(crate) const CURSOR_DEFAULT_DASHBOARD_BASE: &str = "https://api2.cursor.sh";
pub(crate) const CURSOR_SESSION_BASE: &str = "https://cursor.com";

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

/// Selected-account Cursor credential: bearer + derived user id. The user id
/// comes from the JWT `sub` claim (part after `|`), never from config alone.
// No `Debug`: this carries a live access token and must never be formatted
// into a log or error (Claude credentials omit `Debug` for the same reason).
#[derive(Clone)]
pub(crate) struct CursorAuth {
    pub(crate) access_token: String,
    pub(crate) user_id: Option<String>,
}

pub(crate) fn cursor_auth_path() -> PathBuf {
    env_value("CURSOR_CONFIG_DIR").map_or_else(
        || home_path(".cursor/auth.json"),
        |dir| PathBuf::from(dir).join("auth.json"),
    )
}

/// Pure `auth.json` parse: the ambient loader, per-profile snapshots, and the
/// discovery lane mint broker material from a selected profile root through
/// this, so broker refresh never re-resolves the default home for a
/// non-default registered root. `None` is a present-but-tokenless file.
pub(crate) fn cursor_auth_from_value(value: &serde_json::Value) -> Option<CursorAuth> {
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

pub(crate) fn load_cursor_auth() -> Result<CursorAuth, String> {
    let path = cursor_auth_path();
    let value = read_json_file(&path)
        .ok_or_else(|| "Cursor auth.json is missing or unreadable".to_owned())?;
    cursor_auth_from_value(&value).ok_or_else(|| "Cursor access token is missing".to_owned())
}

/// Extract the Cursor user id from a JWT access token: payload `sub`, part
/// after `|`. `None` for opaque (non-JWT) tokens — REST enrichment then stays
/// unavailable rather than guessing an id.
pub(crate) fn cursor_user_id_from_token(token: &str) -> Option<String> {
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

/// Pure `cli-config.json` identity parse (`authInfo`): display label only,
/// never a credential. Shared by ambient loading and discovery so both read
/// the same keys.
pub(crate) fn cursor_cli_identity_from_value(value: &serde_json::Value) -> Option<String> {
    let info = value.get("authInfo")?;
    ["email", "displayName", "display_name", "userId"]
        .into_iter()
        .filter_map(|key| info.get(key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .find(|identity| !identity.is_empty())
        .map(str::to_owned)
}

/// Local CLI identity (`authInfo` in `cli-config.json`): display label only,
/// never a credential.
pub(crate) fn load_cursor_cli_identity() -> Option<String> {
    let path = env_value("CURSOR_CONFIG_DIR").map_or_else(
        || home_path(".cursor/cli-config.json"),
        |dir| PathBuf::from(dir).join("cli-config.json"),
    );
    let value = read_json_file(&path)?;
    cursor_cli_identity_from_value(&value)
}

/// Display label from one `cli-config.json` value (`authInfo`): email first,
/// then display name. Never a credential.
///
/// Same parse as [`cursor_cli_identity_from_value`]; both names are called
/// by `usage/cursor/tests.rs`, so they reconcile together.
pub(crate) fn cursor_identity_from_cli_config(value: &serde_json::Value) -> Option<String> {
    cursor_cli_identity_from_value(value)
}

// ---------------------------------------------------------------------------
// Personal: DashboardService Connect RPC
// ---------------------------------------------------------------------------

pub(crate) fn cursor_dashboard_base() -> String {
    env_value("CURSOR_API_ENDPOINT").unwrap_or_else(|| CURSOR_DEFAULT_DASHBOARD_BASE.to_owned())
}

/// True when enrichment-gated REST calls are allowed: OAuth-file auth against
/// the default base. Custom bases (and API-key auth) skip session enrichment.
pub(crate) fn cursor_default_base() -> bool {
    env_value("CURSOR_API_ENDPOINT").is_none()
}

/// Pure URL join for a dashboard base: the hermetic seam tests use so a live
/// `CURSOR_API_ENDPOINT` can never break (or leak into) assertions.
pub(crate) fn cursor_dashboard_url_with_base(base: &str, method: &str) -> String {
    format!(
        "{}/aiserver.v1.DashboardService/{method}",
        base.trim_end_matches('/')
    )
}
