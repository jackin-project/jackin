// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Secret-bearing Claude credential material passed to broker-owned refreshes.

use zeroize::Zeroizing;

/// Resolved Claude OAuth material for one broker-owned provider refresh.
/// Secret-safe: never `Debug`/`Display`. The access token is held only for the
/// fetch; the opaque account identity is the only credential-derived identity
/// carried into coordination.
#[expect(
    missing_debug_implementations,
    reason = "credential type: the resolved access token must never be formatted into a log or error"
)]
#[derive(Clone)]
pub struct ClaudeResolved {
    access_token: Zeroizing<String>,
    pub subscription_type: Option<String>,
    pub account_email: Option<String>,
    pub organization_type: Option<String>,
    pub credential_origin: String,
    /// `true` when the credential carries no proven cross-account identity.
    pub is_anonymous: bool,
}

impl ClaudeResolved {
    pub(crate) fn access_token(&self) -> &str {
        self.access_token.as_str()
    }

    pub(crate) fn replace_access_token(&mut self, access_token: Zeroizing<String>) {
        self.access_token = access_token;
    }

    /// Construct resolved material for a non-Keychain fixture or adapter.
    #[must_use]
    pub fn from_token(
        access_token: String,
        subscription_type: Option<String>,
        account_email: Option<String>,
        organization_type: Option<String>,
        credential_origin: String,
        is_anonymous: bool,
    ) -> Self {
        Self {
            access_token: Zeroizing::new(access_token),
            subscription_type,
            account_email,
            organization_type,
            credential_origin,
            is_anonymous,
        }
    }

    /// Move parsed OAuth material into a short-lived provider refresh value.
    #[must_use]
    pub fn from_oauth_credentials(
        credential: super::ClaudeOAuthCredentials,
        account_email: Option<String>,
        organization_type: Option<String>,
        credential_origin: String,
        is_anonymous: bool,
    ) -> Self {
        Self {
            access_token: credential.access_token,
            subscription_type: credential.subscription_type,
            account_email,
            organization_type,
            credential_origin,
            is_anonymous,
        }
    }
}

/// Resolved material or local credential failure for one provider view.
///
/// The broker/discovery layer supplies resolved material; this enum does not
/// discover ambient credentials or read user files itself.
#[expect(
    missing_debug_implementations,
    reason = "credential type: the resolved access token must never be formatted into a log or error"
)]
pub enum ClaudeWaveResolution {
    Resolved(Box<ClaudeResolved>),
    Denied,
    Missing,
}
