//! `Agent` enum: the set of AI agents jackin❯ can provision inside a role
//! container.
//!
//! Single source of truth for agent identity — variant ordering, display
//! labels, CLI slug parsing, and serde shape. Every match arm across the
//! codebase that keys on agent identity should use this enum rather than
//! string comparisons.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::auth::AuthForwardMode;
use crate::constants::CLAUDE_OAUTH_TOKEN_ENV;
use crate::env_model;

/// The set of AI agents jackin❯ can provision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    /// Anthropic Claude Code CLI.
    Claude,
    /// `OpenAI` Codex CLI.
    Codex,
    /// Sourcegraph Amp CLI.
    Amp,
    /// Moonshot Kimi Code CLI.
    Kimi,
    /// `OpenCode` CLI.
    Opencode,
    /// xAI Grok Build CLI.
    Grok,
}

impl Agent {
    /// Every variant in declaration order. Iteration sites consult this
    /// instead of hand-rolling their own array.
    pub const ALL: &'static [Self] = &[
        Self::Claude,
        Self::Codex,
        Self::Amp,
        Self::Kimi,
        Self::Opencode,
        Self::Grok,
    ];

    /// Canonical lowercase CLI slug (`"claude"`, `"codex"`, …).
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Amp => "amp",
            Self::Kimi => "kimi",
            Self::Opencode => "opencode",
            Self::Grok => "grok",
        }
    }

    /// Display label shown in TUI surfaces.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Amp => "Amp",
            Self::Kimi => "Kimi",
            Self::Opencode => "OpenCode",
            Self::Grok => "Grok",
        }
    }

    /// Generate the Dockerfile `RUN` block that installs this agent's CLI
    /// from a pre-fetched binary at `source` path.
    pub fn install_block(self, source: &str) -> String {
        self.runtime().install_block(source)
    }

    /// Generate the Dockerfile `RUN` block that installs this agent's CLI
    /// from the official upstream installer when host-side binary prefetch
    /// fails.
    pub fn fallback_install_block(self) -> String {
        self.runtime().fallback_install_block()
    }

    /// Official upstream installer command used when host-side binary prefetch
    /// cannot produce a cached binary.
    pub fn fallback_install_command(self) -> &'static str {
        self.runtime().fallback_install_command()
    }

    /// Well-known env var that carries the auth credential for this
    /// (agent, mode) combination, if any. Returns `None` for modes that
    /// don't inject a credential (sync, ignore) or for combinations that
    /// don't make sense for the agent.
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
            (Self::Claude, M::Sync | M::Ignore)
            | (
                Self::Codex | Self::Amp | Self::Kimi | Self::Opencode | Self::Grok,
                M::Sync | M::Ignore | M::OAuthToken,
            ) => None,
        }
    }

    /// Modes this agent supports. UI surfaces should consult this when
    /// listing options to the user.
    pub const fn supported_modes(self) -> &'static [AuthForwardMode] {
        use AuthForwardMode as M;
        match self {
            Self::Claude => &[M::Sync, M::ApiKey, M::OAuthToken, M::Ignore],
            Self::Codex | Self::Amp | Self::Kimi | Self::Opencode | Self::Grok => {
                &[M::Sync, M::ApiKey, M::Ignore]
            }
        }
    }

    /// Per-agent behavioral dispatch via the [`runtime::AgentRuntime`] trait.
    ///
    /// Returns the adapter that encapsulates all behavioral logic for this
    /// agent. Phase 2 will migrate all match-arm dispatch sites to call
    /// `agent.runtime().<method>()` instead of matching on `agent` directly.
    pub fn runtime(self) -> &'static dyn runtime::AgentRuntime {
        use runtime::adapters;
        match self {
            Self::Claude => &adapters::ClaudeRuntime,
            Self::Codex => &adapters::CodexRuntime,
            Self::Amp => &adapters::AmpRuntime,
            Self::Kimi => &adapters::KimiRuntime,
            Self::Opencode => &adapters::OpencodeRuntime,
            Self::Grok => &adapters::GrokRuntime,
        }
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// Error returned when parsing an agent name fails.
#[derive(Debug, thiserror::Error)]
#[error("unknown agent: {got:?}; supported: claude, codex, amp, kimi, opencode, grok")]
pub struct ParseAgentError {
    got: String,
}

impl Agent {
    /// Parse a canonical agent slug without allocating. Returns `None` on an
    /// unrecognized slug — the hot path (per-process `/proc` sampling) prefers
    /// this over `FromStr`, whose error payload allocates a `String` on every
    /// miss.
    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "amp" => Some(Self::Amp),
            "kimi" => Some(Self::Kimi),
            "opencode" => Some(Self::Opencode),
            "grok" => Some(Self::Grok),
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

pub mod adapters;
pub mod runtime;

#[cfg(test)]
mod tests;
