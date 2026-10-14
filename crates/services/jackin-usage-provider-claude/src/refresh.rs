// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use zeroize::Zeroizing;

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

    pub is_anonymous: bool,
}

impl ClaudeResolved {
    pub(crate) fn access_token(&self) -> &str {
        self.access_token.as_str()
    }

    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn replace_access_token(&mut self, access_token: Zeroizing<String>) {
        self.access_token = access_token;
    }

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

#[expect(
    missing_debug_implementations,
    reason = "credential type: the resolved access token must never be formatted into a log or error"
)]
pub enum ClaudeWaveResolution {
    Resolved(Box<ClaudeResolved>),
    Denied,
    Missing,
}
