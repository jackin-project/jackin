// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::auth::AuthForwardMode;
use crate::constants::CLAUDE_OAUTH_TOKEN_ENV;
use crate::env_model;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,

    Codex,

    Amp,

    Kimi,

    Opencode,

    Grok,

    Antigravity,

    Gemini,

    Cursor,

    Muse,

    Omp,

    Hermes,
}

impl Agent {
    pub const ALL: &'static [Self] = &[
        Self::Claude,
        Self::Codex,
        Self::Amp,
        Self::Kimi,
        Self::Opencode,
        Self::Grok,
        Self::Antigravity,
        Self::Gemini,
        Self::Cursor,
        Self::Muse,
        Self::Omp,
        Self::Hermes,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Amp => "amp",
            Self::Kimi => "kimi",
            Self::Opencode => "opencode",
            Self::Grok => "grok",
            Self::Antigravity => "antigravity",
            Self::Gemini => "gemini",
            Self::Cursor => "cursor",
            Self::Muse => "muse",
            Self::Omp => "omp",
            Self::Hermes => "hermes",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Amp => "Amp",
            Self::Kimi => "Kimi",
            Self::Opencode => "OpenCode",
            Self::Grok => "Grok",
            Self::Antigravity => "Antigravity",
            Self::Gemini => "Gemini",
            Self::Cursor => "Cursor",
            Self::Muse => "Muse",
            Self::Omp => "omp",
            Self::Hermes => "Hermes",
        }
    }

    pub fn install_block(self, source: &str) -> String {
        self.runtime().install_block(source)
    }

    pub fn fallback_install_block(self) -> String {
        self.runtime().fallback_install_block()
    }

    pub fn fallback_install_command(self) -> &'static str {
        self.runtime().fallback_install_command()
    }

    pub const fn required_env_var(self, mode: AuthForwardMode) -> Option<&'static str> {
        use AuthForwardMode as M;
        match (self, mode) {
            (Self::Claude, M::ApiKey) => Some(env_model::ANTHROPIC_API_KEY_ENV_NAME),
            (Self::Claude, M::OAuthToken) => Some(CLAUDE_OAUTH_TOKEN_ENV),
            (Self::Codex, M::ApiKey) => Some(env_model::OPENAI_API_KEY_ENV_NAME),
            (Self::Amp, M::ApiKey) => Some(env_model::AMP_API_KEY_ENV_NAME),
            (Self::Kimi, M::ApiKey) => Some(env_model::KIMI_API_KEY_ENV_NAME),
            (Self::Opencode, M::ApiKey) => Some(env_model::OPENCODE_API_KEY_ENV_NAME),
            (Self::Grok, M::ApiKey) => Some(env_model::XAI_API_KEY_ENV_NAME),
            (Self::Antigravity | Self::Gemini, M::ApiKey) => {
                Some(env_model::GEMINI_API_KEY_ENV_NAME)
            }
            (Self::Cursor, M::ApiKey) => Some(env_model::CURSOR_API_KEY_ENV_NAME),

            (Self::Muse, M::ApiKey) => Some(env_model::META_API_KEY_ENV_NAME),
            (Self::Claude, M::Sync | M::Ignore)
            | (
                Self::Codex
                | Self::Amp
                | Self::Kimi
                | Self::Opencode
                | Self::Grok
                | Self::Antigravity
                | Self::Gemini
                | Self::Cursor
                | Self::Muse,
                M::Sync | M::Ignore | M::OAuthToken,
            )
            | (Self::Omp | Self::Hermes, M::Sync | M::ApiKey | M::Ignore | M::OAuthToken) => None,
        }
    }

    pub const fn supported_modes(self) -> &'static [AuthForwardMode] {
        use AuthForwardMode as M;

        match self {
            Self::Claude => &[M::Sync, M::ApiKey, M::OAuthToken, M::Ignore],
            Self::Codex
            | Self::Amp
            | Self::Kimi
            | Self::Opencode
            | Self::Grok
            | Self::Antigravity
            | Self::Gemini
            | Self::Cursor
            | Self::Muse
            | Self::Omp
            | Self::Hermes => &[M::Sync, M::ApiKey, M::Ignore],
        }
    }

    pub fn runtime(self) -> &'static dyn AgentRuntime {
        use runtime::adapters;
        match self {
            Self::Claude => &adapters::ClaudeRuntime,
            Self::Codex => &adapters::CodexRuntime,
            Self::Amp => &adapters::AmpRuntime,
            Self::Kimi => &adapters::KimiRuntime,
            Self::Opencode => &adapters::OpencodeRuntime,
            Self::Grok => &adapters::GrokRuntime,
            Self::Antigravity => &adapters::AntigravityRuntime,
            Self::Gemini => &adapters::GeminiRuntime,
            Self::Cursor => &adapters::CursorRuntime,
            Self::Muse => &adapters::MuseRuntime,
            Self::Omp => &adapters::OmpRuntime,
            Self::Hermes => &adapters::HermesRuntime,
        }
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

#[derive(Debug)]
pub struct ParseAgentError {
    got: String,
}

impl fmt::Display for ParseAgentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown agent: {:?}; supported: claude, codex, amp, kimi, opencode, grok, antigravity, gemini, cursor, muse, omp, hermes",
            self.got
        )
    }
}

impl std::error::Error for ParseAgentError {}

impl Agent {
    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "amp" => Some(Self::Amp),
            "kimi" => Some(Self::Kimi),
            "opencode" => Some(Self::Opencode),
            "grok" => Some(Self::Grok),
            "antigravity" => Some(Self::Antigravity),
            "gemini" => Some(Self::Gemini),
            "cursor" => Some(Self::Cursor),
            "muse" => Some(Self::Muse),
            "omp" => Some(Self::Omp),
            "hermes" => Some(Self::Hermes),
            _ => None,
        }
    }
}

impl FromStr for Agent {
    type Err = ParseAgentError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_slug(s).ok_or_else(|| ParseAgentError { got: s.to_owned() })
    }
}

pub(crate) mod adapters;
pub(crate) mod runtime;

pub use runtime::{AgentRuntime, AgentStatePaths, FolderVar, FolderVarKind};

#[inline]
#[must_use]
pub const fn agent_runtime_registry() -> &'static [&'static dyn AgentRuntime] {
    adapters::registry()
}

#[cfg(test)]
mod tests;
