// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Claude` OAuth credential loading.

use super::super::{Path, humanize_plan_label, read_json_file};

// No `Debug`/`Display`: this carries a live access token and (optionally) the
// stable refresh token, so it must never be formatted into a log or error.
#[derive(Clone)]
pub(crate) struct ClaudeOAuthCredentials {
    pub(crate) access_token: String,
    pub(crate) subscription_type: Option<String>,
    /// Stable rotation-independent identity input. Consumed only inside wave
    /// resolution to derive the opaque account discriminator, then dropped —
    /// never carried into a view, log, snapshot, or coordination key raw.
    pub(crate) refresh_token: Option<String>,
}

/// Claude account email (F12): `~/.claude.json` carries `oauthAccount` metadata
/// (never the token), and `CodexBar` reads the address from there. Returns the
/// trimmed `oauthAccount.emailAddress`, or `None` when absent.
pub(crate) fn claude_email_from_value(value: &serde_json::Value) -> Option<String> {
    let oauth = value.get("oauthAccount")?;
    oauth
        .get("emailAddress")
        .or_else(|| oauth.get("email_address"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .map(str::to_owned)
}

/// Claude account tier from `oauthAccount.organizationType` in `~/.claude.json`.
///
/// Enterprise/Team accounts store their billing model in `subscriptionType`
/// ("API Usage Billing"), not the account tier. `organizationType` carries the
/// tier directly (e.g. `"claude_enterprise"`, `"claude_max"`, `"claude_team"`) and is
/// the authoritative source for the plan label shown in the TUI header.
pub(crate) fn claude_organization_type_from_value(value: &serde_json::Value) -> Option<String> {
    value
        .get("oauthAccount")?
        .get("organizationType")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(humanize_plan_label)
}

pub(crate) fn load_claude_account_email(path: &Path) -> Option<String> {
    claude_email_from_value(&read_json_file(path)?)
}

#[cfg(test)]
pub(crate) fn load_claude_organization_type(path: &Path) -> Option<String> {
    claude_organization_type_from_value(&read_json_file(path)?)
}

pub(crate) fn claude_oauth_from_value(value: &serde_json::Value) -> Option<ClaudeOAuthCredentials> {
    let oauth = value.get("claudeAiOauth")?;
    let access_token = oauth
        .get("accessToken")
        .or_else(|| oauth.get("access_token"))
        .and_then(serde_json::Value::as_str)?
        .trim()
        .to_owned();
    if access_token.is_empty() {
        return None;
    }
    let subscription_type = oauth
        .get("subscriptionType")
        .or_else(|| oauth.get("subscription_type"))
        .or_else(|| oauth.get("rateLimitTier"))
        .or_else(|| oauth.get("rate_limit_tier"))
        .and_then(serde_json::Value::as_str)
        .map(humanize_plan_label);
    // Optional stable refresh token — used only to derive the coordination
    // discriminator when no `oauthAccount` metadata exists. Never surfaced.
    let refresh_token = oauth
        .get("refreshToken")
        .or_else(|| oauth.get("refresh_token"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_owned);
    Some(ClaudeOAuthCredentials {
        access_token,
        subscription_type,
        refresh_token,
    })
}

#[cfg(test)]
pub(crate) fn load_claude_oauth_credentials(path: &Path) -> Option<ClaudeOAuthCredentials> {
    claude_oauth_from_value(&read_json_file(path)?)
}

// ===================================================================
// macOS Keychain credential source (plan 002)
//
// Claude Code on macOS stores its OAuth credential only in the login
// Keychain (a fresh `/login` deletes the credentials file). The service
// name is derived from the effective `CLAUDE_CONFIG_DIR` by the shared
// `jackin_core::claude_keychain_scope` helper, so instance provisioning and
// this probe never disagree. Rust owns all resolution; Swift is display-only.
// ===================================================================
