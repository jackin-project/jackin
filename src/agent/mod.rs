use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub const fn install_block(self) -> &'static str {
        match self {
            Self::Claude => CLAUDE_INSTALL_BLOCK,
            Self::Codex => CODEX_INSTALL_BLOCK,
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
            (Self::Claude, M::OAuthToken) => Some("CLAUDE_CODE_OAUTH_TOKEN"),
            (Self::Codex, M::ApiKey) => Some("OPENAI_API_KEY"),
            (Self::Claude, M::Sync | M::Ignore)
            | (Self::Codex, M::Sync | M::Ignore | M::OAuthToken) => None,
        }
    }

    /// Modes this agent supports. UI surfaces should consult this when
    /// listing options to the user. The TOML parser uses `CodexAuthConfig`
    /// to reject unsupported modes at parse time — this method is the
    /// runtime/UI parallel.
    pub const fn supported_modes(self) -> &'static [crate::config::AuthForwardMode] {
        use crate::config::AuthForwardMode as M;
        match self {
            Self::Claude => &[M::Sync, M::ApiKey, M::OAuthToken, M::Ignore],
            Self::Codex => &[M::Sync, M::ApiKey, M::Ignore],
        }
    }
}

const CLAUDE_INSTALL_BLOCK: &str = "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN curl -fsSL https://claude.ai/install.sh | bash
RUN claude --version
";

const CODEX_INSTALL_BLOCK: &str = "\
USER root
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
    curl -fsSL \"https://github.com/openai/codex/releases/download/${TAG}/${ASSET}.tar.gz\" \\
      | tar -xzf - -O \"${ASSET}\" > /tmp/codex.bin && \\
    chmod 0755 /tmp/codex.bin && \\
    mv /tmp/codex.bin /usr/local/bin/codex && \\
    mkdir -p /etc/jackin && codex --version > /etc/jackin/codex.version
";

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown agent: {got:?}; supported: claude, codex")]
pub struct ParseAgentError {
    got: String,
}

impl FromStr for Agent {
    type Err = ParseAgentError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
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
        for h in [Agent::Claude, Agent::Codex] {
            assert_eq!(Agent::from_str(h.slug()).unwrap(), h);
        }
    }

    #[test]
    fn display_matches_slug() {
        assert_eq!(format!("{}", Agent::Claude), "claude");
        assert_eq!(format!("{}", Agent::Codex), "codex");
    }

    #[test]
    fn rejects_unknown_agent() {
        let err = Agent::from_str("amp").unwrap_err();
        assert!(err.to_string().contains("amp"));
        assert!(err.to_string().contains("claude"));
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
    fn codex_install_block_installs_cli_as_root_with_current_archive_layout() {
        let block = Agent::Codex.install_block();
        assert!(block.starts_with("USER root\n"));
        assert!(block.contains("set -euxo pipefail"));
        assert!(block.contains("${TARGETARCH:-amd64}"));
        assert!(block.contains("x86_64-unknown-linux-musl"));
        assert!(block.contains("aarch64-unknown-linux-musl"));
        assert!(block.contains("ASSET=\"codex-${ARCH}\""));
        assert!(block.contains("tar -xzf - -O \"${ASSET}\" > /tmp/codex.bin"));
        assert!(block.contains("chmod 0755 /tmp/codex.bin"));
        assert!(block.contains("mv /tmp/codex.bin /usr/local/bin/codex"));
        assert!(block.contains("/etc/jackin/codex.version"));
        assert!(block.contains("openai/codex/releases"));
    }

    #[test]
    fn claude_install_block_installs_cli_via_official_script() {
        let block = Agent::Claude.install_block();
        assert!(block.starts_with("USER agent\n"));
        assert!(block.contains("claude.ai/install.sh"));
        assert!(block.contains("claude --version"));
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
        // OAuthToken for Codex is parser-rejected (Task 6); behavior at the
        // method level is "no env var" (None) for safety.
        assert_eq!(
            Agent::Codex.required_env_var(AuthForwardMode::OAuthToken),
            None
        );
        assert_eq!(Agent::Codex.required_env_var(AuthForwardMode::Ignore), None);
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
}
