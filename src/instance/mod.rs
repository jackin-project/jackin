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
///
/// All host paths land under `/jackin/<agent>/...` inside the
/// container. The agent's expected home-relative paths
/// (`~/.claude.json`, `~/.codex/auth.json`, …) are NOT bind-mounted
/// directly: jackin's entrypoint copies the relevant files from
/// `/jackin/` into the agent's home before launch. This isolates the
/// host→container handoff to a single tree (`/jackin/`) the operator
/// can audit at a glance, and frees `/home/agent/.claude/` and
/// `/home/agent/.codex/` to carry image-baked config (settings.json,
/// hooks/, memory/) without being masked by a runtime mount.
///
/// Path fields hold the host filesystem location regardless of whether
/// the file gets bind-mounted; the mount decision is encoded in
/// `forward_auth` (Claude) or in the optional path fields directly
/// (Codex). `forward_auth = false` means the agent authenticates via
/// env vars (`CLAUDE_CODE_OAUTH_TOKEN` / `ANTHROPIC_API_KEY`) and the
/// auth files must not flow into the container even though they exist
/// on the host (`wipe_claude_state` leaves a `{}` shell behind).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AgentRuntimeState {
    Claude {
        /// Host path mounted at `/jackin/claude/plugins.json:ro`.
        plugins_json: PathBuf,
        /// Host path to Claude's account-metadata file. Always
        /// populated by `prepare` (as `{}` for non-sync modes); only
        /// bind-mounted when `forward_auth` is `true` *and* the file
        /// exists on disk.
        account_json: PathBuf,
        /// Host path to Claude's OAuth credentials file. May not exist
        /// on disk in env-driven modes (the wipe path removes it).
        /// Only bind-mounted when `forward_auth` is `true` *and* the
        /// file exists.
        credentials_json: PathBuf,
        /// Whether `account_json` and `credentials_json` should be
        /// bind-mounted under `/jackin/claude/`. `false` for
        /// `ignore`/`api_key`/`oauth_token` (env-driven authentication
        /// — no host filesystem state must reach the container).
        forward_auth: bool,
    },
    Codex {
        /// Host path mounted at `/jackin/codex/config.toml` (always —
        /// generated from the manifest, not auth state).
        config_toml: PathBuf,
        /// Host path mounted at `/jackin/codex/auth.json` when the
        /// file was synced from the host's `~/.codex/auth.json` on a
        /// previous launch. `None` when the host had no auth.json at
        /// the most recent launch — the bind mount is skipped and any
        /// in-container `codex login` writes to the container's
        /// writable layer (lost on `docker rm`).
        auth_json: Option<PathBuf>,
    },
}

#[derive(Debug, Clone)]
pub struct RoleState {
    pub root: PathBuf,
    pub gh_config_dir: PathBuf,
    pub agent_runtime: AgentRuntimeState,
}

impl RoleState {
    /// Host path to Claude's account-metadata file. `None` when not
    /// prepared for `Agent::Claude`. The path is returned regardless
    /// of mount eligibility — call sites that care whether the file
    /// will reach the container should also consult
    /// [`Self::claude_forwards_auth`].
    #[must_use]
    pub fn claude_account_json(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Claude { account_json, .. } => Some(account_json),
            AgentRuntimeState::Codex { .. } => None,
        }
    }

    /// Host path to Claude's OAuth credentials file. `None` when not
    /// prepared for `Agent::Claude`. As with [`Self::claude_account_json`],
    /// the path is returned regardless of whether the file currently
    /// exists on disk or will be bind-mounted into the container; pair
    /// with [`Self::claude_forwards_auth`] when filtering for runtime
    /// reachability.
    #[must_use]
    pub fn claude_credentials_json(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Claude {
                credentials_json, ..
            } => Some(credentials_json),
            AgentRuntimeState::Codex { .. } => None,
        }
    }

    /// Whether Claude's auth files (`account.json`, `credentials.json`)
    /// should flow into the container under `/jackin/claude/`. `false`
    /// for env-driven modes (`ignore`/`api_key`/`oauth_token`) and for
    /// non-Claude states.
    #[must_use]
    pub const fn claude_forwards_auth(&self) -> bool {
        matches!(
            &self.agent_runtime,
            AgentRuntimeState::Claude {
                forward_auth: true,
                ..
            }
        )
    }

    /// Host path to the Claude plugins manifest (mounted at
    /// `/jackin/claude/plugins.json` in the container). `None` if
    /// this state was not prepared for `Agent::Claude`.
    #[must_use]
    pub fn claude_plugins_json(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Claude { plugins_json, .. } => Some(plugins_json),
            AgentRuntimeState::Codex { .. } => None,
        }
    }

    /// Host path to Codex's `config.toml` (mounted at
    /// `/jackin/codex/config.toml` in the container). `None`
    /// if this state was not prepared for `Agent::Codex`.
    #[must_use]
    pub fn codex_config_toml(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Codex { config_toml, .. } => Some(config_toml),
            AgentRuntimeState::Claude { .. } => None,
        }
    }

    /// Host path to Codex's `auth.json` (mounted at
    /// `/jackin/codex/auth.json` in the container). `None` when
    /// no auth file is available (host had none and no in-container
    /// login has run yet) or when this state was not prepared for
    /// `Agent::Codex`.
    #[must_use]
    pub fn codex_auth_json(&self) -> Option<&Path> {
        match &self.agent_runtime {
            AgentRuntimeState::Codex { auth_json, .. } => auth_json.as_deref(),
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
        let gh_config_dir = root.join(".config/gh");

        std::fs::create_dir_all(&gh_config_dir)?;

        let (agent_runtime, outcome) = match agent {
            crate::agent::Agent::Claude => {
                let claude_dir = root.join("claude");
                std::fs::create_dir_all(&claude_dir)?;

                let account_json = claude_dir.join("account.json");
                let credentials_json = claude_dir.join("credentials.json");
                let plugins_json = claude_dir.join("plugins.json");

                let (outcome, forward_auth) = Self::provision_claude_auth(
                    &account_json,
                    &credentials_json,
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
                (
                    AgentRuntimeState::Claude {
                        plugins_json,
                        account_json,
                        credentials_json,
                        forward_auth,
                    },
                    outcome,
                )
            }
            crate::agent::Agent::Codex => {
                let codex_dir = root.join("codex");
                std::fs::create_dir_all(&codex_dir)?;
                let config_toml = codex_dir.join("config.toml");
                let auth_json_path = codex_dir.join("auth.json");
                let (outcome, auth_json) = Self::provision_codex_auth(
                    &config_toml,
                    &auth_json_path,
                    manifest,
                    auth_forward,
                    host_home,
                )?;
                (
                    AgentRuntimeState::Codex {
                        config_toml,
                        auth_json,
                    },
                    outcome,
                )
            }
        };

        Ok((
            Self {
                root,
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

        // Auth files exist as `{}` placeholders even under env-driven
        // modes; they just won't be bind-mounted (`forward_auth = false`).
        assert_eq!(
            std::fs::read_to_string(state.claude_account_json().unwrap()).unwrap(),
            "{}"
        );
        assert!(
            !state.claude_forwards_auth(),
            "Ignore mode must not forward auth into the container",
        );
        assert!(state.codex_config_toml().is_none());

        // Pin the host-side grouped layout: a regression to the legacy
        // flat shape (`.claude/state/.credentials.json`, `.jackin/plugins.json`
        // at the data-dir root) would still satisfy the accessor checks
        // above, since they only look up paths through the enum. These
        // assertions verify the actual host paths under
        // `<container>/claude/`.
        let container_root = paths.data_dir.join("jackin-agent-smith");
        assert_eq!(
            state.claude_account_json().unwrap(),
            container_root.join("claude").join("account.json"),
        );
        assert_eq!(
            state.claude_credentials_json().unwrap(),
            container_root.join("claude").join("credentials.json"),
        );
        assert_eq!(
            state.claude_plugins_json().unwrap(),
            container_root.join("claude").join("plugins.json"),
        );
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
        assert!(state.claude_account_json().is_none());
        assert!(state.claude_credentials_json().is_none());
        assert!(state.claude_plugins_json().is_none());
        assert!(!state.claude_forwards_auth());
    }
}
