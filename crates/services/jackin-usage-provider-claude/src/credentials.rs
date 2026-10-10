// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Claude` OAuth credential loading.

use jackin_usage_provider_core::{humanize_plan_label, read_json_file};
use serde::Deserialize;
use std::path::Path;
use zeroize::Zeroizing;

// No `Debug`/`Display`: this carries a live access token and (optionally) the
// stable refresh token, so it must never be formatted into a log or error.
#[expect(
    missing_debug_implementations,
    reason = "credential type: live access/refresh tokens must never be formatted into a log or error"
)]
#[derive(Clone)]
pub struct ClaudeOAuthCredentials {
    pub access_token: Zeroizing<String>,
    pub subscription_type: Option<String>,
    /// Stable rotation-independent identity input. Consumed only inside wave
    /// resolution to derive the opaque account discriminator, then dropped —
    /// never carried into a view, log, snapshot, or coordination key raw.
    pub refresh_token: Option<Zeroizing<String>>,
}

struct SecretString(Zeroizing<String>);

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|value| Self(Zeroizing::new(value)))
    }
}

#[derive(Deserialize)]
struct ClaudeCredentialDocument {
    #[serde(rename = "claudeAiOauth", alias = "claude_ai_oauth")]
    oauth: Option<ClaudeCredentialDocumentOauth>,
    #[serde(rename = "oauthAccount", alias = "oauth_account")]
    account: Option<ClaudeCredentialDocumentAccount>,
}

#[derive(Deserialize)]
struct ClaudeCredentialDocumentOauth {
    #[serde(rename = "accessToken", alias = "access_token")]
    access_token: Option<SecretString>,
    #[serde(
        rename = "subscriptionType",
        alias = "subscription_type",
        alias = "rateLimitTier",
        alias = "rate_limit_tier"
    )]
    subscription_type: Option<String>,
    #[serde(rename = "refreshToken", alias = "refresh_token")]
    refresh_token: Option<SecretString>,
}

#[derive(Deserialize)]
struct ClaudeCredentialDocumentAccount {
    #[serde(rename = "emailAddress", alias = "email_address")]
    email_address: Option<String>,
    #[serde(rename = "organizationType", alias = "organization_type")]
    organization_type: Option<String>,
}

/// Parsed Keychain profile. Credential strings enter zeroizing containers
/// during deserialization and never pass through a `serde_json::Value`.
#[expect(
    missing_debug_implementations,
    reason = "credential profile contains live secrets"
)]
pub struct ClaudeKeychainProfile {
    pub credential: Option<ClaudeOAuthCredentials>,
    pub account_email: Option<String>,
    pub organization_type: Option<String>,
}

pub fn parse_claude_keychain_profile(bytes: &[u8]) -> Option<ClaudeKeychainProfile> {
    let document = serde_json::from_slice::<ClaudeCredentialDocument>(bytes).ok()?;
    let account_email = document
        .account
        .as_ref()
        .and_then(|account| account.email_address.as_deref())
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .map(str::to_owned);
    let organization_type = document
        .account
        .as_ref()
        .and_then(|account| account.organization_type.as_deref())
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(humanize_plan_label);
    let credential = document.oauth.and_then(|oauth| {
        let access_token = oauth.access_token?.0;
        if access_token.trim().is_empty() {
            return None;
        }
        let refresh_token = oauth
            .refresh_token
            .map(|token| token.0)
            .filter(|token| !token.trim().is_empty());
        Some(ClaudeOAuthCredentials {
            access_token,
            subscription_type: oauth.subscription_type.as_deref().map(humanize_plan_label),
            refresh_token,
        })
    });
    Some(ClaudeKeychainProfile {
        credential,
        account_email,
        organization_type,
    })
}

/// Claude account email (F12): `~/.claude.json` carries `oauthAccount` metadata
/// (never the token), and `CodexBar` reads the address from there. Returns the
/// trimmed `oauthAccount.emailAddress`, or `None` when absent.
pub fn claude_email_from_value(value: &serde_json::Value) -> Option<String> {
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
pub fn claude_organization_type_from_value(value: &serde_json::Value) -> Option<String> {
    value
        .get("oauthAccount")?
        .get("organizationType")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(humanize_plan_label)
}

pub fn load_claude_account_email(path: &Path) -> Option<String> {
    claude_email_from_value(&read_json_file(path)?)
}

#[cfg(any(test, feature = "test-support"))]
pub fn load_claude_organization_type(path: &Path) -> Option<String> {
    claude_organization_type_from_value(&read_json_file(path)?)
}

pub fn claude_oauth_from_value(value: &serde_json::Value) -> Option<ClaudeOAuthCredentials> {
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
        .map(|token| Zeroizing::new(token.to_owned()));
    Some(ClaudeOAuthCredentials {
        access_token: Zeroizing::new(access_token),
        subscription_type,
        refresh_token,
    })
}

#[cfg(any(test, feature = "test-support"))]
pub fn load_claude_oauth_credentials(path: &Path) -> Option<ClaudeOAuthCredentials> {
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
