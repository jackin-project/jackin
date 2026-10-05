// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

// One declaration supplies both the enum and exhaustive iteration authority.
macro_rules! declare_agents {
    ($($(#[$meta:meta])* $variant:ident => $adapter:ident),* $(,)?) => {
        /// The set of AI agents jackin❯ can provision.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "lowercase")]
        pub enum Agent { $($(#[$meta])* $variant),* }
        impl Agent {
            /// Every variant in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),*];

            pub(crate) const RUNTIME_REGISTRY: &'static [&'static dyn runtime::AgentRuntime] =
                &[$(&runtime::adapters::$adapter),*];

            /// Adapter that encapsulates this agent's behavioral logic.
            pub const fn runtime(self) -> &'static dyn runtime::AgentRuntime {
                match self {
                    $(Self::$variant => &runtime::adapters::$adapter),*
                }
            }
        }
    };
}

declare_agents! {
    /// Anthropic Claude Code CLI.
    Claude => ClaudeRuntime,
    /// `OpenAI` Codex CLI.
    Codex => CodexRuntime,
    /// Sourcegraph Amp CLI.
    Amp => AmpRuntime,
    /// Moonshot Kimi Code CLI.
    Kimi => KimiRuntime,
    /// `OpenCode` CLI.
    Opencode => OpencodeRuntime,
    /// xAI Grok Build CLI.
    Grok => GrokRuntime,
    /// Google Antigravity CLI (`agy`).
    Antigravity => AntigravityRuntime,
    /// Google Gemini CLI (`gemini`).
    Gemini => GeminiRuntime,
    /// Cursor agent CLI (`cursor-agent`, alias `agent`).
    Cursor => CursorRuntime,
    /// Meta Muse CLI (`muse`).
    Muse => MuseRuntime,
    /// oh-my-pi multi-provider client (`omp`).
    Omp => OmpRuntime,
    /// Nous Hermes multi-provider client (`hermes`).
    Hermes => HermesRuntime,
}

impl Agent {
    /// Canonical lowercase CLI slug (`"claude"`, `"codex"`, …).
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

    /// Display label shown in TUI surfaces.
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
            (Self::Antigravity | Self::Gemini, M::ApiKey) => {
                Some(env_model::GEMINI_API_KEY_ENV_NAME)
            }
            (Self::Cursor, M::ApiKey) => Some(env_model::CURSOR_API_KEY_ENV_NAME),
            // Verified in `muse login --help`: META_API_KEY overrides login.
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
            // Omp/Hermes are pure multi-provider clients with no native
            // billing: the ApiKey variable is provider-selected at the
            // account layer (`AccountConfig::api_key_variable`), so no
            // single agent-level variable exists.
            | (Self::Omp | Self::Hermes, M::Sync | M::ApiKey | M::Ignore | M::OAuthToken) => {
                None
            }
        }
    }

    /// Modes this agent supports. UI surfaces should consult this when
    /// listing options to the user.
    pub const fn supported_modes(self) -> &'static [AuthForwardMode] {
        use AuthForwardMode as M;
        // Every agent except Claude supports exactly Sync+ApiKey+Ignore:
        // the six catalog additions follow the same policy (no per-agent
        // OAuthToken flow; Omp/Hermes route provider keys via ApiKey).
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
}

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// Error returned when parsing an agent name fails.
// Hand-written Display/Error: item-level `#[error("...")]` attributes collide
// with the boltffi source scanner (see jackin-usage-ffi).
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

// Nested modules are crate-private; public surface is re-exported below.
pub(crate) mod adapters;
mod launch;
pub(crate) mod runtime;

pub use runtime::{AgentRuntime, AgentStatePaths, FolderVar, FolderVarKind};

/// Public registry entry: all built-in [`AgentRuntime`] adapters.
#[inline]
#[must_use]
pub const fn agent_runtime_registry() -> &'static [&'static dyn AgentRuntime] {
    adapters::registry()
}

#[cfg(test)]
mod tests;
