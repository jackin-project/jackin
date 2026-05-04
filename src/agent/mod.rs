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
