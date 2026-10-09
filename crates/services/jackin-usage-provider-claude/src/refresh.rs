// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Secret-bearing Claude credential material passed to broker-owned refreshes.

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
    pub access_token: String,
    pub subscription_type: Option<String>,
    pub account_email: Option<String>,
    pub organization_type: Option<String>,
    pub credential_origin: String,
    /// `true` when the credential carries no proven cross-account identity.
    pub is_anonymous: bool,
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
