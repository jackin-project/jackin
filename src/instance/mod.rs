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

#[derive(Debug, Clone)]
pub struct RoleState {
    pub root: PathBuf,
    pub claude_dir: PathBuf,
    pub claude_json: PathBuf,
    pub jackin_dir: PathBuf,
    pub plugins_json: PathBuf,
    pub gh_config_dir: PathBuf,
    /// Set only when agent == Codex; the path to the host-side
    /// config.toml that gets mounted at /home/agent/.codex/config.toml.
    pub codex_config_toml: Option<PathBuf>,
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
        let claude_dir = root.join(".claude");
        let claude_json = root.join(".claude.json");
        let jackin_dir = root.join(".jackin");
        let plugins_json = jackin_dir.join("plugins.json");
        let gh_config_dir = root.join(".config/gh");
        let codex_config_toml = root.join("config.toml");

        std::fs::create_dir_all(&claude_dir)?;
        std::fs::create_dir_all(&jackin_dir)?;
        std::fs::create_dir_all(&gh_config_dir)?;

        let outcome = match agent {
            crate::agent::Agent::Claude => {
                let outcome = Self::provision_claude_auth(
                    &claude_json,
                    &claude_dir,
                    auth_forward,
                    host_home,
                )?;

                if let Some(claude_cfg) = manifest.claude.as_ref() {
                    std::fs::write(
                        &plugins_json,
                        serde_json::to_string_pretty(&PluginState {
                            marketplaces: &claude_cfg.marketplaces,
                            plugins: &claude_cfg.plugins,
                        })?,
                    )?;
                }
                outcome
            }
            crate::agent::Agent::Codex => {
                Self::provision_codex_auth(&codex_config_toml, manifest)?;
                AuthProvisionOutcome::Skipped
            }
        };

        let codex_config_toml_field = match agent {
            crate::agent::Agent::Codex => Some(codex_config_toml),
            crate::agent::Agent::Claude => None,
        };

        Ok((
            Self {
                root,
                claude_dir,
                claude_json,
                jackin_dir,
                plugins_json,
                gh_config_dir,
                codex_config_toml: codex_config_toml_field,
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

        assert!(state.claude_dir.is_dir());
        assert_eq!(std::fs::read_to_string(&state.claude_json).unwrap(), "{}");
        assert!(state.codex_config_toml.is_none());
    }

    #[test]
    fn prepares_codex_state_writes_config_toml_and_skips_plugins_json() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        std::fs::write(
            temp.path().join("jackin.role.toml"),
            r#"dockerfile = "Dockerfile"

[agent]
supported = ["codex"]

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
        assert!(state.codex_config_toml.is_some());
        assert!(state.codex_config_toml.as_ref().unwrap().is_file());
        // plugins.json is NOT written for codex.
        assert!(!state.plugins_json.exists());
    }
}
