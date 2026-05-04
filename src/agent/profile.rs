use crate::agent::Agent;

/// Per-agent compile-time data returned by [`profile`].
///
/// Two fields, two consumers — kept deliberately narrow:
///
/// * `install_block` — concatenated into the derived Dockerfile by
///   [`crate::derived_image::render_derived_dockerfile`].
/// * `required_env` — checked at launch by
///   [`crate::runtime::launch::verify_required_agent_env`].
///
/// Earlier revisions also carried `launch_argv`, `installs_plugins`,
/// and `container_state_paths` here, but those were only ever read by
/// this file's own unit tests — `entrypoint.sh` is the actual launch
/// dispatcher and `runtime/launch.rs::agent_mounts` is the actual
/// mount-string source. Keeping decorative-but-unconsumed fields was
/// a maintenance trap (silent drift between profile data and runtime
/// behaviour); they have been removed.
#[derive(Debug, Clone)]
pub struct AgentProfile {
    pub install_block: String,
    pub required_env: Vec<&'static str>,
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
RUN set -eux; \\
    : \"${JACKIN_CACHE_BUST}\"; \\
    case \"${TARGETARCH:-amd64}\" in \\
      amd64) ARCH=x86_64-unknown-linux-musl ;; \\
      arm64) ARCH=aarch64-unknown-linux-musl ;; \\
      *) echo \"unsupported arch ${TARGETARCH}\"; exit 1 ;; \\
    esac; \\
    TAG=$(curl -sfIL -o /dev/null -w '%{url_effective}' \\
            https://github.com/openai/codex/releases/latest \\
          | sed 's|.*/tag/||'); \\
    if [ -z \"${TAG}\" ]; then \\
      echo \"failed to resolve codex release tag — GitHub redirect format may have changed\"; \\
      exit 1; \\
    fi; \\
    case \"${TAG}\" in \\
      v[0-9]*|rust-v[0-9]*) ;; \\
      *) echo \"unexpected codex release tag format: ${TAG}\"; exit 1 ;; \\
    esac; \\
    ASSET=\"codex-${ARCH}\"; \\
    curl -fsSL \"https://github.com/openai/codex/releases/download/${TAG}/${ASSET}.tar.gz\" \\
      | tar -xzf - -O \"${ASSET}\" > /usr/local/bin/codex; \\
    chmod 0755 /usr/local/bin/codex; \\
    mkdir -p /etc/jackin && codex --version > /etc/jackin/codex.version
";

pub fn profile(h: Agent) -> AgentProfile {
    match h {
        Agent::Claude => AgentProfile {
            install_block: CLAUDE_INSTALL_BLOCK.to_string(),
            required_env: vec![],
        },
        Agent::Codex => AgentProfile {
            install_block: CODEX_INSTALL_BLOCK.to_string(),
            required_env: vec!["OPENAI_API_KEY"],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_profile_has_install_block_and_no_required_env() {
        let p = profile(Agent::Claude);
        assert!(p.required_env.is_empty());
        assert!(p.install_block.contains("claude.ai/install.sh"));
    }

    #[test]
    fn codex_profile_requires_openai_key() {
        let p = profile(Agent::Codex);
        assert_eq!(p.required_env, vec!["OPENAI_API_KEY"]);
        assert!(p.install_block.contains("openai/codex/releases"));
        assert!(p.install_block.contains("TARGETARCH"));
    }

    #[test]
    fn codex_profile_installs_cli_as_root_with_current_archive_layout() {
        let p = profile(Agent::Codex);
        assert!(p.install_block.starts_with("USER root\n"));
        assert!(p.install_block.contains("ASSET=\"codex-${ARCH}\""));
        assert!(
            p.install_block
                .contains("tar -xzf - -O \"${ASSET}\" > /usr/local/bin/codex")
        );
        assert!(p.install_block.contains("/etc/jackin/codex.version"));
    }
}
