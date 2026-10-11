// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `UsageSurface` resolution.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageSurface {
    Claude,
    Codex,
    Amp,
    Grok,
    Zai,
    Kimi,
    Minimax,
    OpenCode,
    Cursor,
    Google,
    OpenRouter,
    Unsupported,
}

impl UsageSurface {
    pub(crate) const fn id(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("claude"),
            Self::Codex => Some("codex"),
            Self::Amp => Some("amp"),
            Self::Grok => Some("grok"),
            Self::Zai => Some("zai"),
            Self::Kimi => Some("kimi"),
            Self::Minimax => Some("minimax"),
            Self::OpenCode => Some("opencode"),
            Self::Cursor => Some("cursor"),
            Self::Google => Some("google"),
            Self::OpenRouter => Some("openrouter"),
            Self::Unsupported => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Anthropic",
            Self::Codex => "OpenAI",
            Self::Amp => "Amp",
            Self::Grok => "xAI",
            Self::Zai => "Z.AI",
            Self::Kimi => "Kimi",
            Self::Minimax => "MiniMax",
            Self::OpenCode => "OpenCode",
            Self::Cursor => "Cursor",
            Self::Google => "Google",
            Self::OpenRouter => "OpenRouter",
            Self::Unsupported => "Usage",
        }
    }

    pub(crate) fn account_label(self) -> &'static str {
        match self {
            Self::Claude => "Anthropic",
            Self::Codex => "OpenAI",
            Self::Amp => "Amp",
            Self::Grok => "xAI",
            Self::Zai => "Z.AI",
            Self::Kimi => "Kimi",
            Self::Minimax => "MiniMax",
            Self::OpenCode => "OpenCode",
            Self::Cursor => "Cursor",
            Self::Google => "Google",
            Self::OpenRouter => "OpenRouter",
            Self::Unsupported => "Usage",
        }
    }
}
