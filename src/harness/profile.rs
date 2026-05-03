use crate::harness::Harness;

/// Per-harness data returned by `profile(harness)`.
///
/// Owned types (not `&'static`) so the profile can grow runtime
/// parameterization later without churning consumers. `required_env`
/// keeps `&'static str` because env-var names are inherent literals.
#[derive(Debug, Clone)]
pub struct HarnessProfile {
    pub install_block: String,
    pub launch_argv: Vec<String>,
    pub required_env: Vec<&'static str>,
    pub installs_plugins: bool,
    pub container_state_paths: ContainerStatePaths,
}

#[derive(Debug, Clone)]
pub struct ContainerStatePaths {
    /// Pairs of (path-relative-to-/home/agent, kind).
    pub home_subpaths: Vec<(String, MountKind)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountKind {
    File,
    Dir,
}

const CLAUDE_INSTALL_BLOCK: &str = "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN curl -fsSL https://claude.ai/install.sh | bash
RUN claude --version
";

const CODEX_INSTALL_BLOCK: &str = "\
USER agent
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
    curl -fsSL \"https://github.com/openai/codex/releases/download/${TAG}/codex-${ARCH}.tar.gz\" \\
      | tar -xz -C /usr/local/bin; \\
    chmod +x /usr/local/bin/codex; \\
    mkdir -p /etc/jackin && codex --version > /etc/jackin/codex.version
";

pub fn profile(h: Harness) -> HarnessProfile {
    match h {
        Harness::Claude => HarnessProfile {
            install_block: CLAUDE_INSTALL_BLOCK.to_string(),
            launch_argv: vec![
                "claude".to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--verbose".to_string(),
            ],
            required_env: vec![],
            installs_plugins: true,
            container_state_paths: ContainerStatePaths {
                home_subpaths: vec![
                    (".claude".to_string(), MountKind::Dir),
                    (".claude.json".to_string(), MountKind::File),
                    (".jackin/plugins.json".to_string(), MountKind::File),
                ],
            },
        },
        Harness::Codex => HarnessProfile {
            install_block: CODEX_INSTALL_BLOCK.to_string(),
            launch_argv: vec!["codex".to_string()],
            required_env: vec!["OPENAI_API_KEY"],
            installs_plugins: false,
            container_state_paths: ContainerStatePaths {
                home_subpaths: vec![(".codex/config.toml".to_string(), MountKind::File)],
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_profile_installs_plugins() {
        let p = profile(Harness::Claude);
        assert!(p.installs_plugins);
        assert!(p.required_env.is_empty());
        assert!(p.install_block.contains("claude.ai/install.sh"));
        assert!(p.launch_argv[0] == "claude");
    }

    #[test]
    fn codex_profile_requires_openai_key_and_skips_plugins() {
        let p = profile(Harness::Codex);
        assert!(!p.installs_plugins);
        assert_eq!(p.required_env, vec!["OPENAI_API_KEY"]);
        assert!(p.install_block.contains("openai/codex/releases"));
        assert!(p.install_block.contains("TARGETARCH"));
        assert_eq!(p.launch_argv, vec!["codex"]);
    }

    #[test]
    fn claude_state_paths_match_existing_layout() {
        let p = profile(Harness::Claude);
        let names: Vec<&str> = p
            .container_state_paths
            .home_subpaths
            .iter()
            .map(|(n, _)| n.as_str())
            .collect();
        assert!(names.contains(&".claude"));
        assert!(names.contains(&".claude.json"));
        assert!(names.contains(&".jackin/plugins.json"));
    }

    #[test]
    fn codex_state_paths_only_have_config_toml() {
        let p = profile(Harness::Codex);
        assert_eq!(p.container_state_paths.home_subpaths.len(), 1);
        let (path, kind) = &p.container_state_paths.home_subpaths[0];
        assert_eq!(path, ".codex/config.toml");
        assert_eq!(*kind, MountKind::File);
    }
}
