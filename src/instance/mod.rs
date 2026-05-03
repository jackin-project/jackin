use crate::config::AuthForwardMode;
use crate::manifest::RoleManifest;
use crate::paths::JackinPaths;
use std::path::{Path, PathBuf};

mod auth;
pub mod naming;
mod plugins;

pub use naming::{class_family_matches, next_container_name, primary_container_name, runtime_slug};

use plugins::PluginState;

/// Outcome of the `.claude.json` provisioning step, so callers can surface
/// a one-time notice when host credentials are forwarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthProvisionOutcome {
    /// No host auth was forwarded (ignore mode).
    Skipped,
    /// Host auth was synced (overwritten) into the container state.
    Synced,
    /// Mode would have forwarded, but host file was missing — wrote `{}`.
    HostMissing,
    /// Token mode: empty `.claude.json`, no `.credentials.json` —
    /// Claude Code inside the container uses `CLAUDE_CODE_OAUTH_TOKEN`
    /// from the resolved env.
    TokenMode,
}

/// Agent-specific paths that belong to one variant.
///
/// Encoded as an enum so the agent variant and the actual paths can
/// never disagree — the previous shape (`Option<PathBuf>` plus a
/// runtime invariant "Some iff agent == Codex" enforced by `expect()`
/// across two functions) is now a compile-checked match.
#[derive(Debug, Clone)]
pub enum AgentRuntimeState {
    Claude {
        dir: PathBuf,
        json: PathBuf,
        plugins_json: PathBuf,
    },
    Codex {
        config_toml: PathBuf,
    },
}

#[derive(Debug, Clone)]
pub struct RoleState {
    pub root: PathBuf,
    pub jackin_dir: PathBuf,
    pub gh_config_dir: PathBuf,
    pub agent_runtime: AgentRuntimeState,
}

impl RoleState {
    /// Path to the agent's `.claude/` directory, or `None` if this state
    /// was not prepared for `Agent::Claude`.
    #[must_use]
    pub fn claude_dir(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Claude { dir, .. } => Some(dir),
            AgentRuntimeState::Codex { .. } => None,
        }
    }

    /// Path to the agent's `.claude.json`, or `None` if this state was
    /// not prepared for `Agent::Claude`.
    #[must_use]
    pub fn claude_json(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Claude { json, .. } => Some(json),
            AgentRuntimeState::Codex { .. } => None,
        }
    }

    /// Path to the agent's `plugins.json`, or `None` if this state was
    /// not prepared for `Agent::Claude`.
    #[must_use]
    pub fn plugins_json(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Claude { plugins_json, .. } => Some(plugins_json),
            AgentRuntimeState::Codex { .. } => None,
        }
    }

    /// Path to the agent's `config.toml`, or `None` if this state was
    /// not prepared for `Agent::Codex`.
    #[must_use]
    pub fn codex_config_toml(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Codex { config_toml } => Some(config_toml),
            AgentRuntimeState::Claude { .. } => None,
        }
    }
}

impl RoleState {
    pub fn prepare(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &RoleManifest,
        auth_forward: AuthForwardMode,
        host_home: &Path,
        agent: crate::agent::Agent,
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        let root = paths.data_dir.join(container_name);
        let jackin_dir = root.join(".jackin");
        let gh_config_dir = root.join(".config/gh");

        std::fs::create_dir_all(&jackin_dir)?;
        std::fs::create_dir_all(&gh_config_dir)?;

        let (agent_runtime, outcome) = match agent {
            crate::agent::Agent::Claude => {
                let dir = root.join(".claude");
                let json = root.join(".claude.json");
                let plugins_json = jackin_dir.join("plugins.json");

                std::fs::create_dir_all(&dir)?;

                let outcome = Self::provision_claude_auth(&json, &dir, auth_forward, host_home)?;

                if let Some(claude_cfg) = manifest.claude.as_ref() {
                    std::fs::write(
                        &plugins_json,
                        serde_json::to_string_pretty(&PluginState {
                            marketplaces: &claude_cfg.marketplaces,
                            plugins: &claude_cfg.plugins,
                        })?,
                    )?;
                }
                (
                    AgentRuntimeState::Claude {
                        dir,
                        json,
                        plugins_json,
                    },
                    outcome,
                )
            }
            crate::agent::Agent::Codex => {
                let config_toml = root.join("config.toml");
                Self::provision_codex_auth(&config_toml, manifest)?;
                (
                    AgentRuntimeState::Codex { config_toml },
                    AuthProvisionOutcome::Skipped,
                )
            }
        };

        Ok((
            Self {
                root,
                jackin_dir,
                gh_config_dir,
                agent_runtime,
            },
            outcome,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use tempfile::tempdir;

    fn simple_manifest(temp: &tempfile::TempDir) -> crate::manifest::RoleManifest {
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        crate::manifest::RoleManifest::load(temp.path()).unwrap()
    }

    #[test]
    fn prepares_persisted_claude_state() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest = simple_manifest(&temp);

        let (state, _) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Ignore,
            temp.path(),
            crate::agent::Agent::Claude,
        )
        .unwrap();

        assert!(state.claude_dir().unwrap().is_dir());
        assert_eq!(
            std::fs::read_to_string(state.claude_json().unwrap()).unwrap(),
            "{}"
        );
        assert!(state.codex_config_toml().is_none());
    }

    #[test]
    fn prepares_codex_state_writes_config_toml_and_skips_plugins_json() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#,
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();

        let manifest = RoleManifest::load(temp.path()).unwrap();

        let (state, outcome) = RoleState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Ignore,
            temp.path(),
            crate::agent::Agent::Codex,
        )
        .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::Skipped);
        assert!(state.codex_config_toml().is_some());
        assert!(state.codex_config_toml().unwrap().is_file());
        // Codex state carries no claude/plugins paths — the typed enum
        // makes the absence structural rather than a runtime nil.
        assert!(state.claude_dir().is_none());
        assert!(state.claude_json().is_none());
        assert!(state.plugins_json().is_none());
    }
}
