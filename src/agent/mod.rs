use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Codex,
    Amp,
    Kimi,
    Opencode,
}

impl jackin_console::tui::components::agent_choice::AgentChoice for Agent {
    const ALL: &'static [Self] = Self::ALL;

    fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Amp => "Amp",
            Self::Kimi => "Kimi",
            Self::Opencode => "OpenCode",
        }
    }
}

pub type AgentChoiceState = jackin_console::tui::components::agent_choice::AgentChoiceState<Agent>;

impl Agent {
    /// Every variant in declaration order. Iteration sites consult
    /// this instead of hand-rolling their own array.
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
    /// (agent, mode) combination, if any. Returns None for modes that
    /// don't inject a credential (sync, ignore) or for combinations that
    /// don't make sense for the agent.
    pub const fn required_env_var(
        self,
        mode: crate::config::AuthForwardMode,
    ) -> Option<&'static str> {
        use crate::config::AuthForwardMode as M;
        match (self, mode) {
            (Self::Claude, M::ApiKey) => Some("ANTHROPIC_API_KEY"),
            (Self::Claude, M::OAuthToken) => Some(crate::operator_env::CLAUDE_OAUTH_TOKEN_ENV),
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
    /// listing options to the user. The TOML parser uses agent-specific
    /// newtypes around `AgentAuthConfig` (`CodexAuthConfig`, `AmpAuthConfig`,
    /// `KimiAuthConfig`, `OpencodeAuthConfig`) to reject unsupported modes
    /// at parse time — this method is the runtime/UI parallel.
    pub const fn supported_modes(self) -> &'static [crate::config::AuthForwardMode] {
        use crate::config::AuthForwardMode as M;
        match self {
            Self::Claude => &[M::Sync, M::ApiKey, M::OAuthToken, M::Ignore],
            Self::Codex | Self::Amp | Self::Kimi | Self::Opencode => {
                &[M::Sync, M::ApiKey, M::Ignore]
            }
        }
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

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
                got: other.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_round_trip() {
        for h in Agent::ALL {
            assert_eq!(Agent::from_str(h.slug()).unwrap(), *h);
        }
    }

    #[test]
    fn display_matches_slug() {
        assert_eq!(format!("{}", Agent::Claude), "claude");
        assert_eq!(format!("{}", Agent::Codex), "codex");
        assert_eq!(format!("{}", Agent::Amp), "amp");
        assert_eq!(format!("{}", Agent::Kimi), "kimi");
        assert_eq!(format!("{}", Agent::Opencode), "opencode");
    }

    #[test]
    fn rejects_unknown_agent() {
        let err = Agent::from_str("foo").unwrap_err();
        assert!(err.to_string().contains("foo"));
        assert!(err.to_string().contains("claude"));
        assert!(err.to_string().contains("kimi"));
        assert!(err.to_string().contains("opencode"));
    }

    #[test]
    fn serializes_lowercase() {
        let json = serde_json::to_string(&Agent::Claude).unwrap();
        assert_eq!(json, "\"claude\"");
    }

    #[test]
    fn deserializes_lowercase() {
        let h: Agent = serde_json::from_str("\"codex\"").unwrap();
        assert_eq!(h, Agent::Codex);
    }

    #[test]
    fn codex_install_block_installs_cli_as_agent_with_current_archive_layout() {
        assert_eq!(
            Agent::Codex.install_block(".jackin-runtime/agent-binaries/codex"),
            "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /home/agent/.local/bin
COPY --chown=agent:agent .jackin-runtime/agent-binaries/codex /home/agent/.local/bin/codex
ENV PATH=\"/home/agent/.local/bin:${PATH}\"
RUN set -euxo pipefail && \\
    : \"${JACKIN_CACHE_BUST}\" && \\
    chmod 0755 \"${HOME}/.local/bin/codex\" && \\
    codex --version
"
        );
    }

    #[test]
    fn claude_install_block_installs_cached_cli() {
        assert_eq!(
            Agent::Claude.install_block(".jackin-runtime/agent-binaries/claude"),
            "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /tmp/jackin-agent-binaries
COPY --chown=agent:agent .jackin-runtime/agent-binaries/claude /tmp/jackin-agent-binaries/claude
RUN set -euxo pipefail && \\
    : \"${JACKIN_CACHE_BUST}\" && \\
    chmod 0755 /tmp/jackin-agent-binaries/claude && \\
    /tmp/jackin-agent-binaries/claude install && \\
    claude --version
"
        );
    }

    #[test]
    fn amp_install_block_installs_cached_cli() {
        assert_eq!(
            Agent::Amp.install_block(".jackin-runtime/agent-binaries/amp"),
            "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /home/agent/.amp/bin
COPY --chown=agent:agent .jackin-runtime/agent-binaries/amp /home/agent/.amp/bin/amp
ENV PATH=\"/home/agent/.local/bin:/home/agent/.amp/bin:${PATH}\"
RUN set -euxo pipefail && \\
    : \"${JACKIN_CACHE_BUST}\" && \\
    chmod 0755 \"${HOME}/.amp/bin/amp\" && \\
    mkdir -p \"${HOME}/.local/bin\" && \\
    ln -sf \"${HOME}/.amp/bin/amp\" \"${HOME}/.local/bin/amp\" && \\
    amp --version
"
        );
    }

    #[test]
    fn kimi_install_block_installs_cached_cli() {
        assert_eq!(
            Agent::Kimi.install_block(".jackin-runtime/agent-binaries/kimi"),
            "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /home/agent/.kimi-code/bin
COPY --chown=agent:agent .jackin-runtime/agent-binaries/kimi /home/agent/.kimi-code/bin/kimi
ENV PATH=\"/home/agent/.kimi-code/bin:/home/agent/.local/bin:${PATH}\"
RUN set -euxo pipefail && \\
    : \"${JACKIN_CACHE_BUST}\" && \\
    chmod 0755 \"${HOME}/.kimi-code/bin/kimi\" && \\
    kimi --version
"
        );
    }

    #[test]
    fn opencode_install_block_installs_cached_cli() {
        assert_eq!(
            Agent::Opencode.install_block(".jackin-runtime/agent-binaries/opencode"),
            "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN mkdir -p /home/agent/.opencode/bin
COPY --chown=agent:agent .jackin-runtime/agent-binaries/opencode /home/agent/.opencode/bin/opencode
ENV PATH=\"/home/agent/.opencode/bin:${PATH}\"
RUN set -euxo pipefail && \\
    : \"${JACKIN_CACHE_BUST}\" && \\
    chmod 0755 \"${HOME}/.opencode/bin/opencode\" && \\
    opencode --version
"
        );
    }
}

#[cfg(test)]
mod auth_table_tests {
    use super::*;
    use crate::config::AuthForwardMode;

    #[test]
    fn required_env_var_table() {
        // Claude
        assert_eq!(Agent::Claude.required_env_var(AuthForwardMode::Sync), None);
        assert_eq!(
            Agent::Claude.required_env_var(AuthForwardMode::ApiKey),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(
            Agent::Claude.required_env_var(AuthForwardMode::OAuthToken),
            Some("CLAUDE_CODE_OAUTH_TOKEN")
        );
        assert_eq!(
            Agent::Claude.required_env_var(AuthForwardMode::Ignore),
            None
        );

        // Codex
        assert_eq!(Agent::Codex.required_env_var(AuthForwardMode::Sync), None);
        assert_eq!(
            Agent::Codex.required_env_var(AuthForwardMode::ApiKey),
            Some("OPENAI_API_KEY")
        );
        // OAuthToken for Codex is parser-rejected; behavior at the
        // method level is "no env var" (None) for safety.
        assert_eq!(
            Agent::Codex.required_env_var(AuthForwardMode::OAuthToken),
            None
        );
        assert_eq!(Agent::Codex.required_env_var(AuthForwardMode::Ignore), None);

        // Amp
        assert_eq!(Agent::Amp.required_env_var(AuthForwardMode::Sync), None);
        assert_eq!(
            Agent::Amp.required_env_var(AuthForwardMode::ApiKey),
            Some("AMP_API_KEY")
        );
        // OAuthToken for Amp is parser-rejected; method-level safety returns None.
        assert_eq!(
            Agent::Amp.required_env_var(AuthForwardMode::OAuthToken),
            None
        );
        assert_eq!(Agent::Amp.required_env_var(AuthForwardMode::Ignore), None);

        // Kimi
        assert_eq!(Agent::Kimi.required_env_var(AuthForwardMode::Sync), None);
        assert_eq!(
            Agent::Kimi.required_env_var(AuthForwardMode::ApiKey),
            Some("KIMI_API_KEY")
        );
        assert_eq!(
            Agent::Kimi.required_env_var(AuthForwardMode::OAuthToken),
            None
        );
        assert_eq!(Agent::Kimi.required_env_var(AuthForwardMode::Ignore), None);

        // Opencode
        assert_eq!(
            Agent::Opencode.required_env_var(AuthForwardMode::Sync),
            None
        );
        assert_eq!(
            Agent::Opencode.required_env_var(AuthForwardMode::ApiKey),
            Some("OPENCODE_API_KEY")
        );
        assert_eq!(
            Agent::Opencode.required_env_var(AuthForwardMode::OAuthToken),
            None
        );
        assert_eq!(
            Agent::Opencode.required_env_var(AuthForwardMode::Ignore),
            None
        );
    }

    #[test]
    fn supported_modes_claude_includes_oauth_token() {
        let modes = Agent::Claude.supported_modes();
        assert!(modes.contains(&AuthForwardMode::Sync));
        assert!(modes.contains(&AuthForwardMode::ApiKey));
        assert!(modes.contains(&AuthForwardMode::OAuthToken));
        assert!(modes.contains(&AuthForwardMode::Ignore));
    }

    #[test]
    fn supported_modes_codex_excludes_oauth_token() {
        let modes = Agent::Codex.supported_modes();
        assert!(modes.contains(&AuthForwardMode::Sync));
        assert!(modes.contains(&AuthForwardMode::ApiKey));
        assert!(
            !modes.contains(&AuthForwardMode::OAuthToken),
            "codex must not advertise oauth_token"
        );
        assert!(modes.contains(&AuthForwardMode::Ignore));
    }

    #[test]
    fn supported_modes_amp_excludes_oauth_token() {
        let modes = Agent::Amp.supported_modes();
        assert!(modes.contains(&AuthForwardMode::Sync));
        assert!(modes.contains(&AuthForwardMode::ApiKey));
        assert!(
            !modes.contains(&AuthForwardMode::OAuthToken),
            "amp must not advertise oauth_token"
        );
        assert!(modes.contains(&AuthForwardMode::Ignore));
    }

    #[test]
    fn supported_modes_kimi_excludes_oauth_token() {
        let modes = Agent::Kimi.supported_modes();
        assert!(modes.contains(&AuthForwardMode::Sync));
        assert!(modes.contains(&AuthForwardMode::ApiKey));
        assert!(
            !modes.contains(&AuthForwardMode::OAuthToken),
            "kimi must not advertise oauth_token"
        );
        assert!(modes.contains(&AuthForwardMode::Ignore));
    }

    #[test]
    fn supported_modes_opencode_excludes_oauth_token() {
        let modes = Agent::Opencode.supported_modes();
        assert!(modes.contains(&AuthForwardMode::Sync));
        assert!(modes.contains(&AuthForwardMode::ApiKey));
        assert!(
            !modes.contains(&AuthForwardMode::OAuthToken),
            "opencode must not advertise oauth_token"
        );
        assert!(modes.contains(&AuthForwardMode::Ignore));
    }
}
