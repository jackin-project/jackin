// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Coordinator-side vendor arms for configured-provider credential snapshots.
//!
//! Implements [`CredentialSnapshotVendors`](jackin_usage_credential_snapshots::CredentialSnapshotVendors)
//! over the T3 vendor crates (this T4 crate names them for coordinator-side
//! dispatch) and exposes the historical [`provider_credential_snapshot`] paths.

use std::path::Path;

use jackin_protocol::control::FocusedUsageView;
use jackin_usage_credential_snapshots::CredentialSnapshotVendors;
use jackin_usage_provider_core::{
    GROK_HANDOFF_AUTH_PATH, ProviderError, ProviderFailureMetadata, ProviderRateLimit,
    UsageSurface,
};

use jackin_usage_provider_amp::amp_api_key_snapshot;
use jackin_usage_provider_claude::{
    ClaudeResolved, ClaudeWaveResolution, claude_api_key_snapshot,
    claude_view_from_wave,
};
use jackin_usage_provider_gemini::gemini_snapshot_with_presence;
use jackin_usage_provider_grok::grok_snapshot_from_rpc_result;
use jackin_usage_provider_kimi::kimi_snapshot;
use jackin_usage_provider_minimax::minimax_snapshot;
use jackin_usage_provider_openrouter::openrouter_snapshot_with_rate_limit;
use jackin_usage_provider_zai::provider_key_snapshot;

struct UsageCredentialVendors;

impl CredentialSnapshotVendors for UsageCredentialVendors {
    fn claude_oauth_wave_view(
        &self,
        secret: &str,
        now: i64,
    ) -> (
        FocusedUsageView,
        Option<ProviderRateLimit>,
        Option<ProviderFailureMetadata>,
    ) {
        claude_view_from_wave(
            "claude",
            Some("Claude"),
            now,
            ClaudeWaveResolution::Resolved(Box::new(ClaudeResolved {
                access_token: secret.to_owned(),
                subscription_type: None,
                account_email: None,
                organization_type: None,
                credential_origin: "OAuth · configured source".to_owned(),
                is_anonymous: true,
            })),
        )
    }

    fn claude_api_key_view(&self, key_name: &str, secret: &str, now: i64) -> FocusedUsageView {
        claude_api_key_snapshot("claude", Some("Claude"), key_name, secret, now)
    }

    fn openrouter_key_view(
        &self,
        secret: &str,
        now: i64,
    ) -> (FocusedUsageView, Option<ProviderRateLimit>) {
        openrouter_snapshot_with_rate_limit("opencode", Some(secret), now)
    }

    fn amp_key_view(&self, secret: &str, now: i64) -> FocusedUsageView {
        amp_api_key_snapshot("amp", secret, now)
    }

    fn zai_key_view(&self, key_name: &str, secret: &str, now: i64) -> FocusedUsageView {
        provider_key_snapshot("codex", UsageSurface::Zai, key_name, Some(secret), now)
    }

    fn kimi_key_view(&self, secret: &str, now: i64) -> FocusedUsageView {
        kimi_snapshot("kimi", Some(secret), now)
    }

    fn minimax_key_view(&self, secret: &str, now: i64) -> FocusedUsageView {
        minimax_snapshot("codex", Some(secret), now)
    }

    fn grok_key_view(&self, key_name: &str, now: i64) -> FocusedUsageView {
        grok_snapshot_from_rpc_result(
            "grok",
            now,
            Path::new(GROK_HANDOFF_AUTH_PATH),
            false,
            key_name == jackin_core::XAI_API_KEY_ENV_NAME,
            key_name == jackin_core::GROK_DEPLOYMENT_KEY_ENV_NAME,
            Err(ProviderError::from(
                "Grok billing requires an authenticated profile".to_owned(),
            )),
        )
    }

    fn gemini_key_view(&self, key_name: &str, now: i64) -> FocusedUsageView {
        gemini_snapshot_with_presence(
            "gemini",
            Some("Google"),
            false,
            true,
            &format!("API key · env {key_name}"),
            now,
        )
    }
}

/// Build one explicit configured-provider snapshot while the caller retains the
/// credential. This is the tier-3 probe body used by tier-4 protected-source
/// adapters; the secret is never returned or persisted.
#[must_use]
pub fn provider_credential_snapshot(
    surface_id: &str,
    key_name: &str,
    secret: &str,
) -> FocusedUsageView {
    jackin_usage_credential_snapshots::provider_credential_snapshot(
        surface_id,
        key_name,
        secret,
        &UsageCredentialVendors,
    )
}

pub(crate) fn provider_credential_snapshot_with_metadata(
    surface_id: &str,
    key_name: &str,
    secret: &str,
) -> (
    FocusedUsageView,
    Option<ProviderRateLimit>,
    Option<ProviderFailureMetadata>,
) {
    jackin_usage_credential_snapshots::provider_credential_snapshot_with_metadata(
        surface_id,
        key_name,
        secret,
        &UsageCredentialVendors,
    )
}
