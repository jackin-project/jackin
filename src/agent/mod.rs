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

    pub const fn install_block(self) -> &'static str {
        match self {
            Self::Claude => CLAUDE_INSTALL_BLOCK,
            Self::Codex => CODEX_INSTALL_BLOCK,
            Self::Amp => AMP_INSTALL_BLOCK,
            Self::Kimi => KIMI_INSTALL_BLOCK,
            Self::Opencode => OPENCODE_INSTALL_BLOCK,
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

const CLAUDE_INSTALL_BLOCK: &str = "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN curl -fsSL https://claude.ai/install.sh | bash
RUN claude --version
";

const AMP_INSTALL_BLOCK: &str = "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN curl -fsSL https://ampcode.com/install.sh | bash
RUN amp --version
";

const KIMI_INSTALL_BLOCK: &str = "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN set -euxo pipefail && \\
    : \"${JACKIN_CACHE_BUST}\" && \\
    export PATH=\"${HOME}/.local/bin:${PATH}\" && \\
    curl -fsSL https://code.kimi.com/install.sh | bash && \\
    kimi --version
ENV PATH=\"/home/agent/.local/bin:${PATH}\"
";

const OPENCODE_INSTALL_BLOCK: &str = "\
USER agent\n\
ARG JACKIN_CACHE_BUST=0\n\
RUN set -euo pipefail && \\\n\
    : \"${JACKIN_CACHE_BUST}\" && \\\n\
    case \"$(uname -m)\" in \\\n\
      x86_64)  ARCH=x64 ;; \\\n\
      aarch64) ARCH=arm64 ;; \\\n\
      *) echo \"unsupported arch $(uname -m)\"; exit 1 ;; \\\n\
    esac && \\\n\
    mkdir -p \"${HOME}/.opencode/bin\" && \\\n\
    curl -fsSL \"https://github.com/anomalyco/opencode/releases/latest/download/opencode-linux-${ARCH}.tar.gz\" \\\n\
      | tar xz -C \"${HOME}/.opencode/bin\" && \\\n\
    \"${HOME}/.opencode/bin/opencode\" --version\n\
ENV PATH=\"/home/agent/.opencode/bin:${PATH}\"\n\
";

const CODEX_INSTALL_BLOCK: &str = "\
USER agent
ARG JACKIN_CACHE_BUST=0
ARG TARGETARCH
RUN set -euxo pipefail && \\
    : \"${JACKIN_CACHE_BUST}\" && \\
    case \"${TARGETARCH:-amd64}\" in \\
      amd64) ARCH=x86_64-unknown-linux-musl ;; \\
      arm64) ARCH=aarch64-unknown-linux-musl ;; \\
      *) echo \"unsupported arch ${TARGETARCH}\"; exit 1 ;; \\
    esac && \\
    TAG=$(curl -sfIL -o /dev/null -w '%{url_effective}' \\
            https://github.com/openai/codex/releases/latest \\
          | sed 's|.*/tag/||') && \\
    if [ -z \"${TAG}\" ]; then \\
      echo \"failed to resolve codex release tag — GitHub redirect format may have changed\" && \\
      exit 1; \\
    fi && \\
    case \"${TAG}\" in \\
      v[0-9]*|rust-v[0-9]*) ;; \\
      *) echo \"unexpected codex release tag format: ${TAG}\"; exit 1 ;; \\
    esac && \\
    ASSET=\"codex-${ARCH}\" && \\
    mkdir -p \"${HOME}/.local/bin\" && \\
    curl -fsSL \"https://github.com/openai/codex/releases/download/${TAG}/${ASSET}.tar.gz\" \\
      | tar -xzf - -O \"${ASSET}\" > \"${HOME}/.local/bin/codex\" && \\
    chmod 0755 \"${HOME}/.local/bin/codex\" && \\
    \"${HOME}/.local/bin/codex\" --version
ENV PATH=\"/home/agent/.local/bin:${PATH}\"
";

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
        let block = Agent::Codex.install_block();
        assert!(block.starts_with("USER agent\n"));
        assert!(block.contains("set -euxo pipefail"));
        assert!(block.contains("${TARGETARCH:-amd64}"));
        assert!(block.contains("x86_64-unknown-linux-musl"));
        assert!(block.contains("aarch64-unknown-linux-musl"));
        assert!(block.contains("ASSET=\"codex-${ARCH}\""));
        assert!(block.contains("mkdir -p \"${HOME}/.local/bin\""));
        assert!(block.contains("tar -xzf - -O \"${ASSET}\" > \"${HOME}/.local/bin/codex\""));
        assert!(block.contains("chmod 0755 \"${HOME}/.local/bin/codex\""));
        assert!(block.contains("\"${HOME}/.local/bin/codex\" --version"));
        assert!(block.contains("ENV PATH=\"/home/agent/.local/bin:${PATH}\""));
        assert!(block.contains("openai/codex/releases"));
    }

    #[test]
    fn claude_install_block_installs_cli_via_official_script() {
        let block = Agent::Claude.install_block();
        assert!(block.starts_with("USER agent\n"));
        assert!(block.contains("claude.ai/install.sh"));
        assert!(block.contains("claude --version"));
    }

    #[test]
    fn amp_install_block_installs_cli_via_official_script() {
        let block = Agent::Amp.install_block();
        assert!(block.starts_with("USER agent\n"));
        assert!(block.contains("ampcode.com/install.sh"));
        assert!(block.contains("amp --version"));
    }

    #[test]
    fn kimi_install_block_uses_official_curl_installer() {
        let block = Agent::Kimi.install_block();
        assert!(block.starts_with("USER agent\n"));
        assert!(block.contains("export PATH=\"${HOME}/.local/bin:${PATH}\""));
        assert!(block.contains("curl -fsSL https://code.kimi.com/install.sh | bash"));
        assert!(block.contains("kimi --version"));
        assert!(block.contains("ENV PATH=\"/home/agent/.local/bin:${PATH}\""));
    }

    #[test]
    fn opencode_install_block_downloads_binary_from_github() {
        let block = Agent::Opencode.install_block();
        assert!(block.starts_with("USER agent\n"));
        assert!(block.contains("anomalyco/opencode/releases/latest/download"));
        assert!(block.contains("/opencode\" --version"));
        assert!(block.contains("ENV PATH"));
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
