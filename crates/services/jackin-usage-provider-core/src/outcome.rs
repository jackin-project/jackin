// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

        "antigravity" | "gemini" => UsageSurface::Google,
        _ => UsageSurface::Unsupported,
    }
}

pub fn capability_matches_surface(
    agent: &str,
    provider: Option<&str>,
    capability: &jackin_protocol::usage_broker::UsageAccountCapability,
) -> bool {
    resolve_surface(agent, provider).id() == Some(capability.surface_id.as_str())
}

pub fn split_fetch<U>(result: Option<Result<U, String>>) -> (Option<U>, Option<String>) {
    match result {
        Some(Ok(value)) => (Some(value), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderPresence {
    pub has_data: bool,
    pub has_secret: bool,
}

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
