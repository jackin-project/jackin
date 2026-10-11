// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Credential snapshots and provider outcomes.

use jackin_protocol::control::{UsageConfidence, UsageSnapshotStatus, UsageSource};

use super::UsageSurface;

pub fn resolve_surface(agent: &str, provider: Option<&str>) -> UsageSurface {
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
pub fn capability_matches_surface(
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
pub fn split_fetch<U>(result: Option<Result<U, String>>) -> (Option<U>, Option<String>) {
    match result {
        Some(Ok(value)) => (Some(value), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    }
}

/// Inputs to [`provider_outcome`]. Named fields so the two booleans can't be
/// silently swapped at a call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderPresence {
    pub has_data: bool,
    pub has_secret: bool,
}

/// Lifecycle triad for the simple "API key or nothing" providers: data present →
/// fresh/authoritative; a secret present but no data → unsupported/presence-only;
/// neither → needs-secret. Providers with login/CLI/error nuances (Claude, Codex,
/// Amp, Grok) keep their bespoke logic.
pub fn provider_outcome(
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
