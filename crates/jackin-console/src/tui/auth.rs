// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console auth-tab UI vocabulary.

/// Which auth section the operator is currently editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthKind {
    Claude,
    Codex,
    Amp,
    Kimi,
    Opencode,
    Grok,
    Github,
    /// Z.AI / GLM Coding Plan: env-only auth kind.
    Zai,
    /// `MiniMax` Token Plan: env-only provider credential. Distinct from agent
    /// runtimes; credential lives as `MINIMAX_API_KEY` in `[env]`.
    Minimax,
}

impl AuthKind {
    pub const WORKSPACE_PANEL_KINDS: &'static [Self] = &[
        Self::Claude,
        Self::Codex,
        Self::Amp,
        Self::Opencode,
        Self::Grok,
        Self::Github,
        Self::Zai,
        Self::Minimax,
    ];

    pub const SETTINGS_KINDS: &'static [Self] = &[
        Self::Claude,
        Self::Codex,
        Self::Amp,
        Self::Kimi,
        Self::Opencode,
        Self::Grok,
        Self::Github,
        Self::Zai,
        Self::Minimax,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Amp => "Amp",
            Self::Kimi => "Kimi",
            Self::Opencode => "OpenCode",
            Self::Grok => "Grok",
            Self::Github => "GitHub CLI",
            Self::Zai => "Z.AI",
            Self::Minimax => "MiniMax",
        }
    }

    #[must_use]
    pub const fn supported_modes(self) -> &'static [AuthMode] {
        match self {
            Self::Claude => &[
                AuthMode::Sync,
                AuthMode::ApiKey,
                AuthMode::OAuthToken,
                AuthMode::Ignore,
            ],
            Self::Codex | Self::Amp | Self::Kimi | Self::Opencode | Self::Grok => {
                &[AuthMode::Sync, AuthMode::ApiKey, AuthMode::Ignore]
            }
            Self::Github => &[AuthMode::Sync, AuthMode::Token, AuthMode::Ignore],
            Self::Zai | Self::Minimax => &[AuthMode::ApiKey, AuthMode::Ignore],
        }
    }

    #[must_use]
    pub const fn required_env_var(self, mode: AuthMode) -> Option<&'static str> {
        match (self, mode) {
            (Self::Claude, AuthMode::ApiKey) => {
                Some(jackin_core::env_model::ANTHROPIC_API_KEY_ENV_NAME)
            }
            (Self::Claude, AuthMode::OAuthToken) => {
                Some(jackin_core::env_model::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME)
            }
            (Self::Codex, AuthMode::ApiKey) => {
                Some(jackin_core::env_model::OPENAI_API_KEY_ENV_NAME)
            }
            (Self::Amp, AuthMode::ApiKey) => Some(jackin_core::env_model::AMP_API_KEY_ENV_NAME),
            (Self::Kimi, AuthMode::ApiKey) => {
                Some(jackin_core::env_model::KIMI_CODE_API_KEY_ENV_NAME)
            }
            (Self::Opencode, AuthMode::ApiKey) => {
                Some(jackin_core::env_model::OPENCODE_API_KEY_ENV_NAME)
            }
            (Self::Grok, AuthMode::ApiKey) => Some(jackin_core::env_model::XAI_API_KEY_ENV_NAME),
            (Self::Github, AuthMode::Token) => Some(jackin_core::env_model::GH_TOKEN_ENV_NAME),
            (Self::Zai, AuthMode::ApiKey) => Some(jackin_core::env_model::ZAI_API_KEY_ENV_NAME),
            (Self::Minimax, AuthMode::ApiKey) => {
                Some(jackin_core::env_model::MINIMAX_API_KEY_ENV_NAME)
            }
            _ => None,
        }
    }
}

/// Unified mode enum for the auth-form modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthMode {
    Sync,
    ApiKey,
    OAuthToken,
    Token,
    Ignore,
}

impl AuthMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sync => "sync",
            Self::ApiKey => "api_key",
            Self::OAuthToken => "oauth_token",
            Self::Token => "token",
            Self::Ignore => "ignore",
        }
    }
}

#[must_use]
pub const fn auth_mode_requires_credential(kind: AuthKind, mode: AuthMode) -> bool {
    kind.required_env_var(mode).is_some()
}

#[must_use]
pub const fn auth_mode_supports_source_folder(kind: AuthKind, mode: AuthMode) -> bool {
    matches!(mode, AuthMode::Sync)
        && matches!(
            kind,
            AuthKind::Claude
                | AuthKind::Codex
                | AuthKind::Amp
                | AuthKind::Kimi
                | AuthKind::Opencode
        )
}

#[must_use]
pub const fn can_generate_claude_oauth_token(kind: AuthKind, mode: Option<AuthMode>) -> bool {
    matches!((kind, mode), (AuthKind::Claude, Some(AuthMode::OAuthToken)))
}

#[cfg(test)]
mod tests;
