// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Credential snapshots and provider outcomes.

use std::path::Path;

use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};

use super::{
    ClaudeResolved, ClaudeWaveResolution, GROK_HANDOFF_AUTH_PATH, ProviderRateLimit, UsageSurface,
    UsageViewInput, amp_api_key_snapshot, claude_api_key_snapshot,
    claude_view_from_wave_with_rate_limit, gemini_snapshot_with_presence,
    grok_snapshot_from_rpc_result, kimi_snapshot, minimax_snapshot, now_epoch,
    openrouter_snapshot_with_rate_limit, provider_key_snapshot, refresh, unsupported_snapshot,
    usage_view,
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
            Err(refresh::ProviderError::from(
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

pub(crate) fn resolve_surface(agent: &str, provider: Option<&str>) -> UsageSurface {
    if matches!(
        provider,
        Some("Claude" | "Claude Code" | "Anthropic" | "Anthropic / Claude")
    ) {
        return UsageSurface::Claude;
    }
    if matches!(provider, Some("Codex" | "OpenAI" | "OpenAI / Codex")) {
        return UsageSurface::Codex;
    }
    if matches!(provider, Some("Amp")) {
        return UsageSurface::Amp;
    }
    if matches!(provider, Some("Grok" | "Grok Build" | "xAI" | "xAI / Grok")) {
        return UsageSurface::Grok;
    }
    if matches!(provider, Some("Z.AI" | "GLM" | "GLM / Z.AI")) {
        return UsageSurface::Zai;
    }
    if matches!(provider, Some("Kimi")) {
        return UsageSurface::Kimi;
    }
    if matches!(provider, Some("MiniMax")) {
        return UsageSurface::Minimax;
    }
    if matches!(provider, Some("Cursor")) {
        return UsageSurface::Cursor;
    }
    if matches!(provider, Some("Google" | "Gemini")) {
        return UsageSurface::Google;
    }
    if matches!(provider, Some("OpenRouter")) {
        return UsageSurface::OpenRouter;
    }
    match agent {
        "claude" => UsageSurface::Claude,
        "codex" => UsageSurface::Codex,
        "amp" => UsageSurface::Amp,
        "grok" => UsageSurface::Grok,
        "kimi" => UsageSurface::Kimi,
        "opencode" => UsageSurface::OpenCode,
        "cursor" => UsageSurface::Cursor,
        // `antigravity` shares the Google surface (`HostSurfaceId::from_agent`);
        // `muse`, `omp`, and `hermes` stay explicitly unwired (no pollable
        // fetch), as before.
        "antigravity" | "gemini" => UsageSurface::Google,
        _ => UsageSurface::Unsupported,
    }
}

/// A session capability is an authority for exactly one provider surface. A
/// presentation-tab override must not reuse it under another surface because
/// that would route the refresh and cache entry under the wrong provider.
pub(crate) fn capability_matches_surface(
    agent: &str,
    provider: Option<&str>,
    capability: &jackin_protocol::usage_broker::UsageAccountCapability,
) -> bool {
    resolve_surface(agent, provider).id() == Some(capability.surface_id.as_str())
}

/// Split an optional provider fetch into its `(data, error)` pair: `None` token
/// → no attempt, `Some(Ok)` → data, `Some(Err)` → error. Replaces the
/// `match token { Some => match fetch { … }, None => (None, None) }` boilerplate
/// at every provider fetch site (`token.map(fetch)` feeds this).
pub(crate) fn split_fetch<U>(result: Option<Result<U, String>>) -> (Option<U>, Option<String>) {
    match result {
        Some(Ok(value)) => (Some(value), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    }
}

/// Inputs to [`provider_outcome`]. Named fields so the two booleans can't be
/// silently swapped at a call site.
pub(crate) struct ProviderPresence {
    pub(crate) has_data: bool,
    pub(crate) has_secret: bool,
}

/// Lifecycle triad for the simple "API key or nothing" providers: data present →
/// fresh/authoritative; a secret present but no data → unsupported/presence-only;
/// neither → needs-secret. Providers with login/CLI/error nuances (Claude, Codex,
/// Amp, Grok) keep their bespoke logic.
pub(crate) fn provider_outcome(
    presence: ProviderPresence,
) -> (UsageSnapshotStatus, UsageSource, UsageConfidence) {
    let ProviderPresence {
        has_data,
        has_secret,
    } = presence;
    if has_data {
        (
            UsageSnapshotStatus::Fresh,
            UsageSource::ProviderApi,
            UsageConfidence::Authoritative,
        )
    } else if has_secret {
        (
            UsageSnapshotStatus::Unsupported,
            UsageSource::None,
            UsageConfidence::PresenceOnly,
        )
    } else {
        (
            UsageSnapshotStatus::NeedsSecret,
            UsageSource::None,
            UsageConfidence::None,
        )
    }
}
