//! `Agent` enum: the set of AI agents jackin' can provision inside a role
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

/// The set of AI agents jackin' can provision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Codex,
    Amp,
    Kimi,
    Opencode,
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
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Amp => "amp",
            Self::Kimi => "kimi",
            Self::Opencode => "opencode",
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
        }
    }

    /// Generate the Dockerfile `RUN` block that installs this agent's CLI
    /// from a pre-fetched binary at `source` path.
    pub fn install_block(self, source: &str) -> String {
        match self {
            Self::Claude => format!(
                "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /tmp/jackin-agent-binaries
COPY --chown=agent:agent {source} /tmp/jackin-agent-binaries/claude
RUN set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    chmod 0755 /tmp/jackin-agent-binaries/claude && \\
    /tmp/jackin-agent-binaries/claude install && \\
    claude --version
"
            ),
            Self::Codex => format!(
                "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /home/agent/.local/bin
COPY --chown=agent:agent {source} /home/agent/.local/bin/codex
ENV PATH=\"/home/agent/.local/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    chmod 0755 \"${{HOME}}/.local/bin/codex\" && \\
    codex --version
"
            ),
            Self::Amp => format!(
                "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /home/agent/.amp/bin
COPY --chown=agent:agent {source} /home/agent/.amp/bin/amp
ENV PATH=\"/home/agent/.local/bin:/home/agent/.amp/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    chmod 0755 \"${{HOME}}/.amp/bin/amp\" && \\
    mkdir -p \"${{HOME}}/.local/bin\" && \\
    ln -sf \"${{HOME}}/.amp/bin/amp\" \"${{HOME}}/.local/bin/amp\" && \\
    amp --version
"
            ),
            Self::Kimi => format!(
                "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /home/agent/.kimi-code/bin
COPY --chown=agent:agent {source} /home/agent/.kimi-code/bin/kimi
ENV PATH=\"/home/agent/.kimi-code/bin:/home/agent/.local/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    chmod 0755 \"${{HOME}}/.kimi-code/bin/kimi\" && \\
    kimi --version
"
            ),
            Self::Opencode => format!(
                "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /home/agent/.opencode/bin
COPY --chown=agent:agent {source} /home/agent/.opencode/bin/opencode
ENV PATH=\"/home/agent/.opencode/bin:${{PATH}}\"
RUN set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    chmod 0755 \"${{HOME}}/.opencode/bin/opencode\" && \\
    opencode --version
"
            ),
        }
    }

    /// Well-known env var that carries the auth credential for this
    /// (agent, mode) combination, if any. Returns `None` for modes that
    /// don't inject a credential (sync, ignore) or for combinations that
    /// don't make sense for the agent.
    pub const fn required_env_var(self, mode: AuthForwardMode) -> Option<&'static str> {
        use AuthForwardMode as M;
        match (self, mode) {
            (Self::Claude, M::ApiKey) => Some("ANTHROPIC_API_KEY"),
            (Self::Claude, M::OAuthToken) => Some(CLAUDE_OAUTH_TOKEN_ENV),
            (Self::Codex, M::ApiKey) => Some("OPENAI_API_KEY"),
            (Self::Amp, M::ApiKey) => Some("AMP_API_KEY"),
            (Self::Kimi, M::ApiKey) => Some("KIMI_API_KEY"),
            (Self::Opencode, M::ApiKey) => Some("OPENCODE_API_KEY"),
            (Self::Claude, M::Sync | M::Ignore)
            | (
                Self::Codex | Self::Amp | Self::Kimi | Self::Opencode,
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
            Self::Codex | Self::Amp | Self::Kimi | Self::Opencode => {
                &[M::Sync, M::ApiKey, M::Ignore]
            }
        }
    }

    /// Per-agent behavioral dispatch via the [`AgentRuntime`] trait.
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
#[error("unknown agent: {got:?}; supported: claude, codex, amp, kimi, opencode")]
pub struct ParseAgentError {
    got: String,
}

impl FromStr for Agent {
    type Err = ParseAgentError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "amp" => Ok(Self::Amp),
            "kimi" => Ok(Self::Kimi),
            "opencode" => Ok(Self::Opencode),
            other => Err(ParseAgentError {
                got: other.to_owned(),
            }),
        }
    }
}

pub mod adapters;
pub mod runtime;

#[cfg(test)]
mod auth_table_tests;
#[cfg(test)]
mod tests;
