// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Configured-provider credential snapshots (vendor dispatch).

use std::path::Path;

use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{
    GROK_HANDOFF_AUTH_PATH, ProviderError, ProviderRateLimit, UsageSurface, UsageViewInput,
    now_epoch, unsupported_snapshot, usage_view,
};

use super::{
    ClaudeResolved, ClaudeWaveResolution, amp_api_key_snapshot, claude_api_key_snapshot,
    claude_view_from_wave_with_rate_limit, gemini_snapshot_with_presence,
    grok_snapshot_from_rpc_result, kimi_snapshot, minimax_snapshot,
    openrouter_snapshot_with_rate_limit, provider_key_snapshot,
};

/// Build one explicit configured-provider snapshot while the caller retains the
/// credential. This is the tier-3 probe body used by tier-4 protected-source
/// adapters; the secret is never returned or persisted.
#[must_use]
pub fn provider_credential_snapshot(
    surface_id: &str,
    key_name: &str,
    secret: &str,
) -> FocusedUsageView {
    provider_credential_snapshot_with_rate_limit(surface_id, key_name, secret).0
}

pub(crate) fn provider_credential_snapshot_with_rate_limit(
    surface_id: &str,
    key_name: &str,
    secret: &str,
) -> (FocusedUsageView, Option<ProviderRateLimit>) {
    let now = now_epoch();
    if surface_id == "claude" {
        if key_name == jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME {
            return claude_view_from_wave_with_rate_limit(
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
            );
        }
        return (
            claude_api_key_snapshot("claude", Some("Claude"), key_name, secret, now),
            None,
        );
    }
    if surface_id == "openrouter" {
        return openrouter_snapshot_with_rate_limit("opencode", Some(secret), now);
    }
    let view = match surface_id {
        "amp" => amp_api_key_snapshot("amp", secret, now),
        "zai" => provider_key_snapshot("codex", UsageSurface::Zai, key_name, Some(secret), now),
        "kimi" => kimi_snapshot("kimi", Some(secret), now),
        "minimax" => minimax_snapshot("codex", Some(secret), now),
        "grok" => grok_snapshot_from_rpc_result(
            "grok",
            now,
            Path::new(GROK_HANDOFF_AUTH_PATH),
            false,
            key_name == jackin_core::XAI_API_KEY_ENV_NAME,
            key_name == jackin_core::GROK_DEPLOYMENT_KEY_ENV_NAME,
            Err(ProviderError::from(
                "Grok billing requires an authenticated profile".to_owned(),
            )),
        ),
        "codex" => usage_view(UsageViewInput {
            agent: "codex",
            provider: Some("OpenAI"),
            surface: UsageSurface::Codex,
            account_label: String::new(),
            username: None,
            plan_label: None,
            credential_origin: Some("API key · configured source".to_owned()),
            buckets: Vec::new(),
            status: UsageSnapshotStatus::Unsupported,
            source: UsageSource::None,
            confidence: UsageConfidence::None,
            now,
            last_error: Some("OpenAI API-key subscription quota is unavailable".to_owned()),
        }),
        // A Cursor API key cannot drive the personal dashboard RPC (that
        // needs the OAuth JWT from `auth.json`); the key route stays an
        // explicit gap like the OpenAI arm above, never a speculative fetch.
        "cursor" => usage_view(UsageViewInput {
            agent: "cursor",
            provider: Some("Cursor"),
            surface: UsageSurface::Cursor,
            account_label: String::new(),
            username: None,
            plan_label: None,
            credential_origin: Some("API key · configured source".to_owned()),
            buckets: Vec::new(),
            status: UsageSnapshotStatus::Unsupported,
            source: UsageSource::None,
            confidence: UsageConfidence::None,
            now,
            last_error: Some("Cursor API-key subscription quota is unavailable".to_owned()),
        }),
        "google" => gemini_snapshot_with_presence(
            "gemini",
            Some("Google"),
            false,
            true,
            &format!("API key · env {key_name}"),
            now,
        ),
        // Explicitly blocked (no production dispatch): `meta` (Muse has no
        // pollable usage fetch by design), `omp`/`hermes`
        // (attribution-only adapters with no native endpoint), `copilot` (no
        // collector, registry, or discovery entry exists at all).
        _ => unsupported_snapshot(surface_id, None, now),
    };
    (view, None)
}
