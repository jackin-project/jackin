use crate::config::AuthForwardMode;
use crate::manifest::AgentManifest;
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
pub struct AgentState {
    pub root: PathBuf,
    pub claude_dir: PathBuf,
    pub claude_json: PathBuf,
    pub jackin_dir: PathBuf,
    pub plugins_json: PathBuf,
    pub gh_config_dir: PathBuf,
}

impl AgentState {
    pub fn prepare(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &AgentManifest,
        auth_forward: AuthForwardMode,
        host_home: &Path,
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        let root = paths.data_dir.join(container_name);
        let claude_dir = root.join(".claude");
        let claude_json = root.join(".claude.json");
        let jackin_dir = root.join(".jackin");
        let plugins_json = jackin_dir.join("plugins.json");
        let gh_config_dir = root.join(".config/gh");

        std::fs::create_dir_all(&claude_dir)?;
        std::fs::create_dir_all(&jackin_dir)?;
        std::fs::create_dir_all(&gh_config_dir)?;

        let outcome =
            Self::provision_claude_auth(&claude_json, &claude_dir, auth_forward, host_home)?;

        std::fs::write(
            &plugins_json,
            serde_json::to_string_pretty(&PluginState {
                marketplaces: &manifest.claude.marketplaces,
                plugins: &manifest.claude.plugins,
            })?,
        )?;

        Ok((
            Self {
                root,
                claude_dir,
                claude_json,
                jackin_dir,
                plugins_json,
                gh_config_dir,
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

    fn simple_manifest(temp: &tempfile::TempDir) -> crate::manifest::AgentManifest {
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
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
        crate::manifest::AgentManifest::load(temp.path()).unwrap()
    }

    #[test]
    fn prepares_persisted_claude_state() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest = simple_manifest(&temp);

        let (state, _) = AgentState::prepare(
            &paths,
            "jackin-agent-smith",
            &manifest,
            AuthForwardMode::Ignore,
            temp.path(),
        )
        .unwrap();

        assert!(state.claude_dir.is_dir());
        assert_eq!(std::fs::read_to_string(&state.claude_json).unwrap(), "{}");
    }
}
