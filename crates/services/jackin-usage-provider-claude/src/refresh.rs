// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Claude` refresh-wave resolution.

use super::{
    ClaudeKeychainRead, ClaudeKeychainState, ClaudeOAuthCredentials, claude_oauth_from_value,
};

/// Result of resolving the Claude credential for one refresh wave. Secret-safe:
/// never `Debug`/`Display`. The access token rides in `Resolved` for the fetch;
/// the opaque `discriminator` is the only identity carried into coordination.
#[expect(
    missing_debug_implementations,
    reason = "credential type: the resolved access token must never be formatted into a log or error"
)]
pub enum ClaudeWaveResolution {
    Resolved(Box<ClaudeResolved>),
    /// Operator denied the Keychain consent for this service — terminal,
    /// local-only. No file/env read, no cached-quota restoration.
    Denied,
    /// No usable credential from Keychain or fallback. Local-only needs-login.
    Missing,
}

#[expect(
    missing_debug_implementations,
    reason = "credential type: the resolved access token must never be formatted into a log or error"
)]
#[derive(Clone)]
pub struct ClaudeResolved {
    pub access_token: String,
    pub subscription_type: Option<String>,
    pub account_email: Option<String>,
    pub organization_type: Option<String>,
    pub credential_origin: String,
    /// `true` when the credential carries no proven cross-account identity (no
    /// account metadata and no refresh token) — a local-only credential.
    pub is_anonymous: bool,
}

/// One credential candidate probe result: the parsed OAuth credential (if any)
/// plus same-scope account/tier metadata.
#[expect(
    missing_debug_implementations,
    reason = "credential type: the probed OAuth credential must never be formatted into a log or error"
)]
pub struct ClaudeFileProbe {
    pub credential: Option<ClaudeOAuthCredentials>,
    pub origin: Option<String>,
    pub account_email: Option<String>,
    pub organization_type: Option<String>,
}

/// Resolve the Claude wave for `scope`: Keychain first, then scope-appropriate
/// file/env fallback. `keychain_reader` performs the real (or test) Keychain
/// read; `file_probe` returns the scope's file credential + metadata in one
/// call; `env_reader` yields an OAuth env token. No process-global env
/// mutation — all inputs are injected so the whole path is unit-testable.
pub fn resolve_claude_refresh_wave_with<K, P, E>(
    scope: &jackin_core::ClaudeKeychainScope,
    state: &ClaudeKeychainState,
    keychain_reader: K,
    file_probe: P,
    env_reader: E,
) -> ClaudeWaveResolution
where
    K: FnOnce(&str) -> ClaudeKeychainRead,
    P: FnOnce() -> ClaudeFileProbe,
    E: FnOnce() -> Option<ClaudeOAuthEnvToken>,
{
    match state.read_with(&scope.service, keychain_reader) {
        ClaudeKeychainRead::Denied => ClaudeWaveResolution::Denied,
        #[cfg(any(target_os = "macos", test))]
        ClaudeKeychainRead::Payload { json } => {
            match serde_json::from_str::<serde_json::Value>(&json)
                .ok()
                .as_ref()
                .and_then(claude_oauth_from_value)
            {
                Some(credential) => {
                    // Valid Keychain payload: may still collect account/tier
                    // metadata from the same-scope file probe, but the file
                    // credential can never replace the Keychain one.
                    let probe = file_probe();
                    let origin = format!("OAuth · macOS Keychain ({})", scope.service);
                    ClaudeWaveResolution::Resolved(Box::new(claude_resolved(
                        credential,
                        origin,
                        probe.account_email,
                        probe.organization_type,
                    )))
                }
                None => resolve_claude_fallback(scope, file_probe(), env_reader()),
            }
        }
        ClaudeKeychainRead::Missing | ClaudeKeychainRead::ConsentRequired => {
            resolve_claude_fallback(scope, file_probe(), env_reader())
        }
    }
}

fn resolve_claude_fallback(
    scope: &jackin_core::ClaudeKeychainScope,
    probe: ClaudeFileProbe,
    env_token: Option<ClaudeOAuthEnvToken>,
) -> ClaudeWaveResolution {
    if let Some(credential) = probe.credential {
        let origin = probe
            .origin
            .unwrap_or_else(|| "OAuth · credentials file".to_owned());
        return ClaudeWaveResolution::Resolved(Box::new(claude_resolved(
            credential,
            origin,
            probe.account_email,
            probe.organization_type,
        )));
    }
    let _ = scope;
    if let Some(token) = env_token {
        return ClaudeWaveResolution::Resolved(Box::new(ClaudeResolved {
            access_token: token.0,
            subscription_type: None,
            account_email: probe.account_email,
            organization_type: probe.organization_type,
            credential_origin: format!(
                "OAuth · env {}",
                jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME
            ),
            is_anonymous: true,
        }));
    }
    ClaudeWaveResolution::Missing
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeOAuthEnvToken(String);

impl ClaudeOAuthEnvToken {
    pub fn new(value: String) -> Self {
        Self(value)
    }
}

fn claude_resolved(
    credential: ClaudeOAuthCredentials,
    origin: String,
    account_email: Option<String>,
    organization_type: Option<String>,
) -> ClaudeResolved {
    // Identity is proven by same-scope account metadata or the stable refresh
    // token; a rotating access token is never identity. Without either, the
    // credential is anonymous (local-only, no cross-account coordination).
    let is_anonymous = !(account_email
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
        || credential
            .refresh_token
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty()));
    ClaudeResolved {
        access_token: credential.access_token,
        subscription_type: credential.subscription_type,
        account_email,
        organization_type,
        credential_origin: origin,
        is_anonymous,
    }
}
