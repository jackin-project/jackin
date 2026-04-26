use crate::workspace::WorkspaceConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use crate::workspace::MountConfig;
pub use crate::workspace::WorkspaceAgentOverride;

mod agents;
pub mod editor;
mod mounts;
mod persist;
mod workspaces;

pub use editor::{ConfigEditor, EnvScope};
pub use mounts::{DockerMounts, MountEntry};

/// Serde helper: `skip_serializing_if` requires `fn(&T) -> bool`.
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(v: &bool) -> bool {
    !*v
}

/// Controls how the host's `~/.claude.json` is forwarded into agent containers.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthForwardMode {
    /// Revoke any forwarded auth and never copy — container starts with `{}`.
    Ignore,
    /// Overwrite container auth from host on each launch when host auth
    /// exists; preserve container auth when host auth is absent.
    #[default]
    Sync,
    /// Use a long-lived OAuth token from the operator-resolved env
    /// (`CLAUDE_CODE_OAUTH_TOKEN`). The agent state directory is
    /// provisioned empty (same shape as `Ignore`); Claude Code inside
    /// the container picks up the token from its process environment.
    Token,
}

impl std::fmt::Display for AuthForwardMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ignore => write!(f, "ignore"),
            Self::Sync => write!(f, "sync"),
            Self::Token => write!(f, "token"),
        }
    }
}

impl std::str::FromStr for AuthForwardMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // `"copy"` is kept as a separate arm (rather than merged with `"sync"`)
        // so callers can pattern-match the literal when emitting a deprecation
        // warning before calling `parse()`.
        #[allow(clippy::match_same_arms)]
        match s {
            "ignore" => Ok(Self::Ignore),
            "sync" => Ok(Self::Sync),
            "token" => Ok(Self::Token),
            // Deprecated alias — accepted to avoid breaking scripts and
            // configs from before the default flipped to `sync`. Callers
            // that want to surface the deprecation should check for the
            // literal `"copy"` themselves before calling `parse()`.
            "copy" => Ok(Self::Sync),
            other => Err(format!(
                "invalid auth_forward mode {other:?}; expected one of: sync, ignore, token"
            )),
        }
    }
}

impl<'de> serde::Deserialize<'de> for AuthForwardMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(D::Error::custom)
    }
}

/// Global Claude Code configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaudeConfig {
    #[serde(default)]
    pub auth_forward: AuthForwardMode,
}

/// Per-agent Claude Code configuration override.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaudeAgentConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_forward: Option<AuthForwardMode>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentSource {
    pub git: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub trusted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<ClaudeAgentConfig>,
    /// Agent-layer operator env map. Merged on top of the global
    /// `[env]` map when the agent is launched. Values use the
    /// `operator_env` dispatch syntax.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerConfig {
    #[serde(default)]
    pub mounts: DockerMounts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub claude: ClaudeConfig,
    /// Global operator env map — the bottom layer. Merged under
    /// per-agent, per-workspace, and per-(workspace × agent) layers.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub agents: BTreeMap<String, AgentSource>,
    #[serde(default)]
    pub docker: DockerConfig,
    #[serde(default)]
    pub workspaces: BTreeMap<String, WorkspaceConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use tempfile::tempdir;

    #[test]
    fn deserializes_scoped_docker_mounts() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "~/.chainargos/secrets", dst = "/secrets", readonly = true }

[docker.mounts."chainargos/agent-brown"]
brown-config = { src = "~/.chainargos/brown", dst = "/config" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let mounts = &config.docker.mounts;
        match mounts.get("chainargos/*").unwrap() {
            MountEntry::Scoped(scope) => {
                let m = scope.get("chainargos-secrets").unwrap();
                assert_eq!(m.dst, "/secrets");
                assert!(m.readonly);
            }
            MountEntry::Mount(_) => panic!("expected MountEntry::Scoped"),
        }
        match mounts.get("chainargos/agent-brown").unwrap() {
            MountEntry::Scoped(scope) => {
                let m = scope.get("brown-config").unwrap();
                assert_eq!(m.dst, "/config");
                assert!(!m.readonly);
            }
            MountEntry::Mount(_) => panic!("expected MountEntry::Scoped"),
        }
    }

    #[test]
    fn deserializes_saved_workspaces() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/Users/donbeave/Projects/chainargos/big-monorepo"
default_agent = "agent-smith"
allowed_agents = ["agent-smith", "chainargos/the-architect"]

[[workspaces.big-monorepo.mounts]]
src = "/Users/donbeave/Projects/chainargos/big-monorepo"
dst = "/Users/donbeave/Projects/chainargos/big-monorepo"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/cache"
dst = "/workspace/cache"
readonly = true
"#;

        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let workspace = config.workspaces.get("big-monorepo").unwrap();

        assert_eq!(
            workspace.workdir,
            "/Users/donbeave/Projects/chainargos/big-monorepo"
        );
        assert_eq!(workspace.mounts.len(), 2);
        assert_eq!(workspace.default_agent.as_deref(), Some("agent-smith"));
        assert_eq!(workspace.allowed_agents.len(), 2);
        assert!(workspace.mounts[1].readonly);
    }

    #[test]
    fn rejects_workspace_with_workdir_outside_mounts() {
        let temp = tempdir().unwrap();

        let workspace = crate::workspace::WorkspaceConfig {
            workdir: "/workspace/project".to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: temp.path().display().to_string(),
                dst: "/workspace/src".to_string(),
                readonly: false,
            }],
            ..Default::default()
        };

        let error =
            crate::workspace::validate_workspace_config("big-monorepo", &workspace).unwrap_err();

        assert!(error.to_string().contains(
            "must be equal to, inside, or a parent of one of the workspace mount destinations"
        ));
    }

    #[test]
    fn edit_workspace_does_not_persist_invalid_mutation() {
        use crate::workspace::WorkspaceEdit;
        let temp = tempdir().unwrap();
        let mut config = AppConfig::default();
        let src = temp.path().display().to_string();

        config
            .create_workspace(
                "big-monorepo",
                WorkspaceConfig {
                    workdir: "/workspace/project".to_string(),
                    mounts: vec![MountConfig {
                        src,
                        dst: "/workspace/project".to_string(),
                        readonly: false,
                    }],
                    ..Default::default()
                },
            )
            .unwrap();

        let error = config
            .edit_workspace(
                "big-monorepo",
                WorkspaceEdit {
                    workdir: Some("/workspace/missing".to_string()),
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        assert!(error.to_string().contains(
            "must be equal to, inside, or a parent of one of the workspace mount destinations"
        ));
        assert_eq!(
            config.workspaces.get("big-monorepo").unwrap().workdir,
            "/workspace/project"
        );
    }

    #[test]
    fn load_or_init_rejects_invalid_saved_workspace() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        std::fs::write(
            &paths.config_file,
            r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp"
dst = "/workspace/src"
"#,
        )
        .unwrap();

        let error = AppConfig::load_or_init(&paths).unwrap_err();

        assert!(error.to_string().contains(
            "must be equal to, inside, or a parent of one of the workspace mount destinations"
        ));
    }

    #[test]
    fn load_or_init_rejects_invalid_persisted_workspace() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mount_src = temp.path().join("workspace-src");
        std::fs::create_dir_all(&mount_src).unwrap();

        let toml_str = format!(
            r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.broken]
workdir = "/workspace/project"

[[workspaces.broken.mounts]]
src = "{}"
dst = "/workspace/src"
"#,
            mount_src.display()
        );

        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, toml_str).unwrap();

        let err = AppConfig::load_or_init(&paths).unwrap_err();
        assert!(err.to_string().contains("workspace \"broken\" workdir must be equal to, inside, or a parent of one of the workspace mount destinations"));
    }

    #[test]
    fn set_agent_auth_forward_creates_claude_section() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#;
        let mut config: AppConfig = toml::from_str(toml_str).unwrap();
        config.set_agent_auth_forward("agent-smith", AuthForwardMode::Sync);
        assert_eq!(
            config.resolve_auth_forward_mode("agent-smith"),
            AuthForwardMode::Sync
        );
    }

    #[test]
    fn existing_config_without_claude_section_deserializes_with_defaults() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.claude.auth_forward, AuthForwardMode::Sync);
        assert_eq!(
            config.resolve_auth_forward_mode("agent-smith"),
            AuthForwardMode::Sync
        );
    }

    #[test]
    fn auth_forward_mode_from_str_accepts_token() {
        use std::str::FromStr;
        assert_eq!(
            AuthForwardMode::from_str("token").unwrap(),
            AuthForwardMode::Token
        );
    }

    #[test]
    fn auth_forward_mode_display_emits_token() {
        assert_eq!(AuthForwardMode::Token.to_string(), "token");
    }

    #[test]
    fn auth_forward_mode_deserializes_token() {
        let toml_str = r#"
[claude]
auth_forward = "token"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.claude.auth_forward, AuthForwardMode::Token);
    }

    #[test]
    fn auth_forward_mode_from_str_error_lists_token() {
        use std::str::FromStr;
        let err = AuthForwardMode::from_str("nope").unwrap_err();
        assert!(
            err.contains("token"),
            "error message should advertise the token mode; got: {err}"
        );
    }

    #[test]
    fn edit_workspace_rejects_upsert_that_introduces_child_under_existing_parent() {
        use crate::workspace::{MountConfig, WorkspaceConfig, WorkspaceEdit};

        let mut config = AppConfig::default();
        config
            .create_workspace(
                "test",
                WorkspaceConfig {
                    workdir: "/a".into(),
                    mounts: vec![MountConfig {
                        src: "/a".into(),
                        dst: "/a".into(),
                        readonly: false,
                    }],
                    ..Default::default()
                },
            )
            .unwrap();

        let err = config
            .edit_workspace(
                "test",
                WorkspaceEdit {
                    upsert_mounts: vec![MountConfig {
                        src: "/a/b".into(),
                        dst: "/a/b".into(),
                        readonly: false,
                    }],
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("already covered") || msg.contains("redundant"),
            "expected 'already covered' or 'redundant' in error message, got: {msg}"
        );
    }

    #[test]
    fn edit_workspace_rejects_upsert_with_readonly_mismatch_vs_existing_child() {
        use crate::workspace::{MountConfig, WorkspaceConfig, WorkspaceEdit};

        let mut config = AppConfig::default();
        config
            .create_workspace(
                "test",
                WorkspaceConfig {
                    workdir: "/a/b".into(),
                    mounts: vec![MountConfig {
                        src: "/a/b".into(),
                        dst: "/a/b".into(),
                        readonly: true,
                    }],
                    ..Default::default()
                },
            )
            .unwrap();

        let err = config
            .edit_workspace(
                "test",
                WorkspaceEdit {
                    upsert_mounts: vec![MountConfig {
                        src: "/a".into(),
                        dst: "/a".into(),
                        readonly: false,
                    }],
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        assert!(err.to_string().contains("readonly"));
    }

    #[test]
    fn edit_workspace_accepts_pre_collapsed_upsert_that_replaces_children() {
        // CLI's job is to pre-collapse. Here we simulate it: instead of
        // upserting just the parent (which would leave children as redundants
        // and fail the post-condition), the CLI removes the children via
        // remove_destinations AND upserts the parent in the same edit.
        use crate::workspace::{MountConfig, WorkspaceConfig, WorkspaceEdit};

        let mut config = AppConfig::default();
        config
            .create_workspace(
                "test",
                WorkspaceConfig {
                    workdir: "/a/b".into(),
                    mounts: vec![
                        MountConfig {
                            src: "/a/b".into(),
                            dst: "/a/b".into(),
                            readonly: false,
                        },
                        MountConfig {
                            src: "/a/c".into(),
                            dst: "/a/c".into(),
                            readonly: false,
                        },
                    ],
                    ..Default::default()
                },
            )
            .unwrap();

        config
            .edit_workspace(
                "test",
                WorkspaceEdit {
                    upsert_mounts: vec![MountConfig {
                        src: "/a".into(),
                        dst: "/a".into(),
                        readonly: false,
                    }],
                    remove_destinations: vec!["/a/b".into(), "/a/c".into()],
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap();

        let ws = config
            .list_workspaces()
            .into_iter()
            .find(|(n, _)| *n == "test")
            .map(|(_, w)| w)
            .expect("workspace should exist");
        assert_eq!(ws.mounts.len(), 1);
        assert_eq!(ws.mounts[0].src, "/a");
    }

    #[test]
    fn edit_workspace_rejects_leaving_pre_existing_violation() {
        // A workspace already containing a rule-C violation. An unrelated edit
        // (e.g., adding an allowed agent) should be blocked by the post-check.
        use crate::workspace::{MountConfig, WorkspaceConfig, WorkspaceEdit};

        let mut config = AppConfig::default();
        config.insert_workspace_raw(
            "legacy",
            WorkspaceConfig {
                workdir: "/a".into(),
                mounts: vec![
                    MountConfig {
                        src: "/a".into(),
                        dst: "/a".into(),
                        readonly: false,
                    },
                    MountConfig {
                        src: "/a/b".into(),
                        dst: "/a/b".into(),
                        readonly: false,
                    },
                ],
                ..Default::default()
            },
        );

        let err = config
            .edit_workspace(
                "legacy",
                WorkspaceEdit {
                    allowed_agents_to_add: vec!["agent-x".into()],
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("redundant") || msg.contains("already covered"),
            "expected 'redundant' or 'already covered' in error message, got: {msg}"
        );
    }

    #[test]
    fn create_workspace_errors_on_child_under_parent_in_initial_mounts() {
        use crate::workspace::{MountConfig, WorkspaceConfig};

        let mut config = AppConfig::default();
        let err = config
            .create_workspace(
                "test",
                WorkspaceConfig {
                    workdir: "/a".into(),
                    mounts: vec![
                        MountConfig {
                            src: "/a".into(),
                            dst: "/a".into(),
                            readonly: false,
                        },
                        MountConfig {
                            src: "/a/b".into(),
                            dst: "/a/b".into(),
                            readonly: false,
                        },
                    ],
                    ..Default::default()
                },
            )
            .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("redundant") || msg.contains("already covered"),
            "expected 'redundant' or 'already covered' in error message, got: {msg}"
        );
    }

    #[test]
    fn create_workspace_errors_on_readonly_mismatch_in_initial_mounts() {
        use crate::workspace::{MountConfig, WorkspaceConfig};

        let mut config = AppConfig::default();
        let err = config
            .create_workspace(
                "test",
                WorkspaceConfig {
                    workdir: "/a".into(),
                    mounts: vec![
                        MountConfig {
                            src: "/a".into(),
                            dst: "/a".into(),
                            readonly: false,
                        },
                        MountConfig {
                            src: "/a/b".into(),
                            dst: "/a/b".into(),
                            readonly: true,
                        },
                    ],
                    ..Default::default()
                },
            )
            .unwrap_err();

        assert!(err.to_string().contains("readonly"));
    }

    #[test]
    fn create_workspace_accepts_already_collapsed_mount_set() {
        use crate::workspace::{MountConfig, WorkspaceConfig};

        let mut config = AppConfig::default();
        config
            .create_workspace(
                "test",
                WorkspaceConfig {
                    workdir: "/a".into(),
                    mounts: vec![MountConfig {
                        src: "/a".into(),
                        dst: "/a".into(),
                        readonly: false,
                    }],
                    ..Default::default()
                },
            )
            .unwrap();
    }

    #[test]
    fn auth_forward_mode_default_is_sync() {
        assert_eq!(AuthForwardMode::default(), AuthForwardMode::Sync);
    }

    #[test]
    fn auth_forward_mode_from_str_accepts_copy_as_deprecated_alias() {
        use std::str::FromStr;
        assert_eq!(
            AuthForwardMode::from_str("copy").unwrap(),
            AuthForwardMode::Sync
        );
    }

    #[test]
    fn auth_forward_mode_from_str_accepts_sync_and_ignore() {
        use std::str::FromStr;
        assert_eq!(
            AuthForwardMode::from_str("sync").unwrap(),
            AuthForwardMode::Sync
        );
        assert_eq!(
            AuthForwardMode::from_str("ignore").unwrap(),
            AuthForwardMode::Ignore
        );
    }

    #[test]
    fn auth_forward_mode_from_str_rejects_unknown_values() {
        use std::str::FromStr;
        assert!(AuthForwardMode::from_str("bogus").is_err());
    }

    #[test]
    fn auth_forward_mode_deserializes_copy_to_sync() {
        let toml_str = r#"
[claude]
auth_forward = "copy"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.claude.auth_forward, AuthForwardMode::Sync);
    }

    #[test]
    fn auth_forward_mode_display_does_not_emit_copy() {
        assert_eq!(AuthForwardMode::Sync.to_string(), "sync");
        assert_eq!(AuthForwardMode::Ignore.to_string(), "ignore");
    }

    #[test]
    fn deserializes_global_env_map() {
        let toml_str = r#"
[env]
OPERATOR_GLOBAL = "literal"
OPERATOR_SECRET = "op://Personal/api/token"
OPERATOR_HOST = "$HOME_VAR"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.env.get("OPERATOR_GLOBAL").unwrap(), "literal");
        assert_eq!(
            config.env.get("OPERATOR_SECRET").unwrap(),
            "op://Personal/api/token"
        );
        assert_eq!(config.env.get("OPERATOR_HOST").unwrap(), "$HOME_VAR");
    }

    #[test]
    fn deserializes_per_agent_env_map() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[agents.agent-smith.env]
AGENT_TOKEN = "op://Shared/smith/token"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let agent = config.agents.get("agent-smith").unwrap();
        assert_eq!(
            agent.env.get("AGENT_TOKEN").unwrap(),
            "op://Shared/smith/token"
        );
    }

    #[test]
    fn deserializes_per_workspace_env_map() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.env]
WORKSPACE_VAR = "literal"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let ws = config.workspaces.get("big-monorepo").unwrap();
        assert_eq!(ws.env.get("WORKSPACE_VAR").unwrap(), "literal");
    }

    #[test]
    fn deserializes_workspace_agent_override_env() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.agents.agent-smith.env]
PER_WORKSPACE_PER_AGENT = "specific"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let ws = config.workspaces.get("big-monorepo").unwrap();
        let override_ = ws.agents.get("agent-smith").unwrap();
        assert_eq!(
            override_.env.get("PER_WORKSPACE_PER_AGENT").unwrap(),
            "specific"
        );
    }

    #[test]
    fn env_maps_default_to_empty_when_omitted() {
        let toml_str = r#"
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert!(config.env.is_empty());
        assert!(config.agents.get("agent-smith").unwrap().env.is_empty());
    }

    #[test]
    fn deserializes_agent_with_slash_in_name_using_quoted_keys() {
        // The spec calls out `[agents."chainargos/agent-jones".env]`
        // and `[workspaces.<ws>.agents."chainargos/agent-jones".env]`
        // as the TOML shape for third-party agent selectors that
        // include a `/`. Standard TOML quoted keys suffice — this
        // test locks in that shape so a future refactor does not
        // accidentally require un-quoted identifiers.
        let toml_str = r#"
[agents."chainargos/agent-jones"]
git = "https://github.com/chainargos/jackin-agent-jones.git"

[agents."chainargos/agent-jones".env]
DATABASE_URL = "op://Work/agent-jones/db"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.agents."chainargos/agent-jones".env]
OPENAI_API_KEY = "op://Work/big-monorepo/OpenAI"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let agent = config.agents.get("chainargos/agent-jones").unwrap();
        assert_eq!(
            agent.env.get("DATABASE_URL").unwrap(),
            "op://Work/agent-jones/db"
        );
        let ws = config.workspaces.get("big-monorepo").unwrap();
        let override_ = ws.agents.get("chainargos/agent-jones").unwrap();
        assert_eq!(
            override_.env.get("OPENAI_API_KEY").unwrap(),
            "op://Work/big-monorepo/OpenAI"
        );
    }
}
