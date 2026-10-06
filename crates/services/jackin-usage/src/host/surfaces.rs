// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Host surface identity and descriptors.

use jackin_core::Agent;

/// Surfaces the host menu bar may show (excludes `Unsupported`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HostSurfaceId {
    /// Anthropic / `Claude`.
    Claude,
    /// `OpenAI` / `Codex`.
    Codex,
    /// Amp.
    Amp,
    /// xAI / Grok Build.
    Grok,
    /// GLM / Z.AI routed provider.
    Zai,
    /// Kimi.
    Kimi,
    /// `MiniMax` routed provider.
    Minimax,
    /// `OpenCode`.
    OpenCode,
    /// Google (Antigravity + Gemini CLI).
    Google,
    /// Cursor.
    Cursor,
    /// Meta (Muse).
    Meta,
    /// `OpenRouter` (multi-provider clients only).
    OpenRouter,
}

impl HostSurfaceId {
    /// Every host surface in stable UI order.
    pub const ALL: &'static [Self] = &[
        Self::Codex,
        Self::Claude,
        Self::Amp,
        Self::Grok,
        Self::Zai,
        Self::Kimi,
        Self::Minimax,
        Self::OpenCode,
        Self::Google,
        Self::Cursor,
        Self::Meta,
        Self::OpenRouter,
    ];

    /// The canonical seven-provider Desktop glance order (Capsule tab order).
    /// `OpenCode` is intentionally excluded from the Desktop item contract.
    pub const DESKTOP_PROVIDER_ORDER: &'static [Self] = &[
        Self::Codex,
        Self::Claude,
        Self::Amp,
        Self::Grok,
        Self::Zai,
        Self::Kimi,
        Self::Minimax,
    ];

    /// Stable machine id (`claude`, `codex`, `zai`, …).
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Amp => "amp",
            Self::Grok => "grok",
            Self::Zai => "zai",
            Self::Kimi => "kimi",
            Self::Minimax => "minimax",
            Self::OpenCode => "opencode",
            Self::Google => "google",
            Self::Cursor => "cursor",
            Self::Meta => "meta",
            Self::OpenRouter => "openrouter",
        }
    }

    /// Canonical provider identity, separate from legacy agent routing ids.
    #[must_use]
    pub const fn provider_id(self) -> &'static str {
        match self {
            Self::Claude => "anthropic",
            Self::Codex => "openai",
            Self::Amp => "amp",
            Self::Grok => "xai",
            Self::Zai => "zai",
            Self::Kimi => "kimi",
            Self::Minimax => "minimax",
            Self::OpenCode => "opencode",
            Self::Google => "google",
            Self::Cursor => "cursor",
            Self::Meta => "meta",
            Self::OpenRouter => "openrouter",
        }
    }

    /// Human label matching Capsule usage tabs.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Anthropic",
            Self::Codex => "OpenAI",
            Self::Amp => "Amp",
            Self::Grok => "xAI",
            Self::Zai => "Z.AI",
            Self::Kimi => "Kimi",
            Self::Minimax => "MiniMax",
            Self::OpenCode => "OpenCode",
            Self::Google => "Google",
            Self::Cursor => "Cursor",
            Self::Meta => "Meta",
            Self::OpenRouter => "OpenRouter",
        }
    }

    /// Two-character menu-bar prefix for the compact status item (HIG width).
    #[must_use]
    pub const fn compact_prefix(self) -> &'static str {
        match self {
            Self::Claude => "Cl",
            Self::Codex => "Cx",
            Self::Amp => "Am",
            Self::Grok => "Gr",
            Self::Zai => "ZA",
            Self::Kimi => "Ki",
            Self::Minimax => "MM",
            Self::OpenCode => "OC",
            Self::Google => "Go",
            Self::Cursor => "Cu",
            Self::Meta => "Me",
            Self::OpenRouter => "OR",
        }
    }

    /// Canonical provider label used by durable account-key hashing.
    #[must_use]
    pub const fn account_provider_label(self) -> &'static str {
        self.label()
    }

    /// Rust-owned fallback glyph used only when the native icon cannot load.
    #[must_use]
    pub const fn fallback_glyph(self) -> &'static str {
        self.compact_prefix()
    }

    /// Provider-owned usage/settings destination for Desktop actions.
    #[must_use]
    pub const fn usage_url(self) -> Option<&'static str> {
        match self {
            Self::Codex => Some("https://chatgpt.com/codex/settings/usage"),
            Self::Claude => Some("https://claude.ai/settings/usage"),
            Self::Amp => Some("https://ampcode.com/settings"),
            Self::Grok => Some("https://console.x.ai/team/default/usage"),
            Self::Zai => Some("https://z.ai/manage-apikey/coding-plan/personal/usage"),
            Self::Kimi => Some("https://www.kimi.com/membership/subscription?tab=quota"),
            Self::Minimax => Some("https://platform.minimax.io/console/usage"),
            Self::OpenCode => None,
            Self::Google => Some("https://aistudio.google.com/usage"),
            Self::Cursor => Some("https://cursor.com/settings"),
            Self::Meta => None,
            Self::OpenRouter => Some("https://openrouter.ai/activity"),
        }
    }

    /// Agent slug used by shared presentation helpers.
    #[must_use]
    pub const fn agent_slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Amp => "amp",
            Self::Grok => "grok",
            Self::Zai | Self::Minimax => "codex",
            Self::Kimi => "kimi",
            Self::OpenCode => "opencode",
            Self::Google => "gemini",
            Self::Cursor => "cursor",
            Self::Meta => "muse",
            Self::OpenRouter => "opencode",
        }
    }

    /// Optional provider label for surface resolution.
    #[must_use]
    pub const fn provider_label(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("Anthropic"),
            Self::Codex => Some("OpenAI"),
            Self::Amp => Some("Amp"),
            Self::Grok => Some("xAI"),
            Self::Zai => Some("Z.AI"),
            Self::Kimi => Some("Kimi"),
            Self::Minimax => Some("MiniMax"),
            Self::OpenCode => Some("OpenCode"),
            Self::Google => Some("Google"),
            Self::Cursor => Some("Cursor"),
            Self::Meta => Some("Meta"),
            Self::OpenRouter => Some("OpenRouter"),
        }
    }

    /// Parse a stable id; unknown → `None`.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|surface| surface.id() == id)
    }

    /// Parse an enumerated provider alias into exact ownership.
    ///
    /// This deliberately does not inspect [`Self::agent_slug`]: Z.AI and
    /// `MiniMax` route through the Codex probe but never own `OpenAI` accounts.
    #[must_use]
    pub fn from_provider_alias(alias: &str) -> Option<Self> {
        let normalized = alias
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        match normalized.as_str() {
            "claude" | "anthropic" | "anthropicclaude" => Some(Self::Claude),
            "codex" | "openai" | "openaicodex" => Some(Self::Codex),
            "amp" => Some(Self::Amp),
            "grok" | "grokbuild" | "xai" | "xaigrok" => Some(Self::Grok),
            "zai" | "glm" | "glmzai" => Some(Self::Zai),
            "kimi" | "moonshot" => Some(Self::Kimi),
            "minimax" => Some(Self::Minimax),
            "opencode" => Some(Self::OpenCode),
            "google" | "gemini" | "antigravity" => Some(Self::Google),
            "cursor" => Some(Self::Cursor),
            "meta" | "muse" => Some(Self::Meta),
            "openrouter" => Some(Self::OpenRouter),
            _ => None,
        }
    }

    /// Map jackin agent runtimes to their primary surface (not Z.AI/MiniMax).
    #[must_use]
    pub const fn from_agent(agent: Agent) -> Self {
        match agent {
            Agent::Claude => Self::Claude,
            Agent::Codex => Self::Codex,
            Agent::Amp => Self::Amp,
            Agent::Kimi => Self::Kimi,
            Agent::Opencode => Self::OpenCode,
            Agent::Grok => Self::Grok,
            Agent::Antigravity | Agent::Gemini => Self::Google,
            Agent::Cursor => Self::Cursor,
            Agent::Muse => Self::Meta,
            // Omp/Hermes are multi-provider clients with no native surface;
            // they share the generic multi-provider surface until per-provider
            // routing lands in the usage lane.
            Agent::Omp | Agent::Hermes => Self::OpenCode,
        }
    }
}

/// Descriptor returned to `boltffi` / CLI (no secrets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSurfaceDescriptor {
    /// Stable id (`claude`).
    pub id: String,
    /// Display label (for example `Claude`).
    pub label: String,
    /// Agent slug used for probes.
    pub agent: String,
    /// Provider label when set.
    pub provider: Option<String>,
    /// Whether the surface is currently enabled for refresh/bar.
    pub enabled: bool,
}
