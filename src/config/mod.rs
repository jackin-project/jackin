use crate::workspace::WorkspaceConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use crate::workspace::MountConfig;
pub use crate::workspace::WorkspaceRoleOverride;

pub mod editor;
pub(crate) mod migrations;
mod mounts;
pub(crate) mod persist;
mod roles;
mod workspaces;

pub use editor::{ConfigEditor, EnvScope};
pub use migrations::{
    CURRENT_CONFIG_VERSION, CURRENT_WORKSPACE_VERSION, migrate_config_file_if_needed,
    migrate_workspace_file_if_needed,
};
pub(crate) use mounts::MountEntry;
pub use mounts::{DockerMounts, GlobalMountRow, WorkspaceGlobalMountRows};
pub use roles::{build_github_env_layers, resolve_github_mode, resolve_mode};
pub use workspaces::{DriftDetection, detect_workspace_edit_drift};

/// Serde helper: `skip_serializing_if` requires `fn(&T) -> bool`.
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(v: &bool) -> bool {
    !*v
}

/// Controls how the host's agent credentials are forwarded into role containers.
///
/// Wire format (TOML / JSON) uses explicit per-variant `rename` so the names
/// the operator types match what `serde` reads. Without `rename`, the default
/// `snake_case` rule turns `OAuthToken` into `o_auth_token`, which is not
/// what we want.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthForwardMode {
    /// Overwrite container auth from host on each launch when host auth
    /// exists; preserve container auth when host auth is absent.
    #[default]
    #[serde(rename = "sync")]
    Sync,
    /// Use a short-lived API key sourced from the operator-resolved env
    /// (e.g. `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `AMP_API_KEY`). The role state
    /// directory is provisioned empty; the agent inside the container
    /// reads the key from its process environment.
    #[serde(rename = "api_key")]
    ApiKey,
    /// Use a long-lived OAuth token sourced from the operator-resolved env
    /// (e.g. `CLAUDE_CODE_OAUTH_TOKEN`). The role state directory is
    /// provisioned empty; the agent inside the container reads the token
    /// from its process environment.
    #[serde(rename = "oauth_token")]
    OAuthToken,
    /// Revoke any forwarded auth and never copy — container starts with `{}`.
    #[serde(rename = "ignore")]
    Ignore,
}

impl std::fmt::Display for AuthForwardMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sync => write!(f, "sync"),
            Self::ApiKey => write!(f, "api_key"),
            Self::OAuthToken => write!(f, "oauth_token"),
            Self::Ignore => write!(f, "ignore"),
        }
    }
}

impl std::str::FromStr for AuthForwardMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sync" => Ok(Self::Sync),
            "api_key" => Ok(Self::ApiKey),
            "oauth_token" => Ok(Self::OAuthToken),
            "ignore" => Ok(Self::Ignore),
            other => Err(format!(
                "invalid auth_forward mode {other:?}; expected one of: sync, api_key, oauth_token, ignore"
            )),
        }
    }
}

/// Per-agent auth configuration wrapper.
///
/// Used at every layer (global, per-role, per-workspace, per-(workspace × role))
/// to carry the auth-forwarding mode for a particular agent.
///
/// Credentials always live in the shared `[env]` block at the same layer.
/// For `auth_forward = "oauth_token"` mode, `jackin workspace claude-token
/// setup` writes `CLAUDE_CODE_OAUTH_TOKEN` into `[workspaces.<ws>.env]`
/// as an `op://` reference — identical to how `ANTHROPIC_API_KEY` or
/// `GH_TOKEN` are stored.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentAuthConfig {
    #[serde(default)]
    pub auth_forward: AuthForwardMode,
}

/// Controls how the host's `gh` auth state reaches role containers.
///
/// Distinct from [`AuthForwardMode`] because GitHub has no `api_key`
/// / `oauth_token` distinction — `gh` PATs are uniform regardless of
/// how the operator obtained them.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GithubAuthMode {
    /// Materialize `~/.config/gh/hosts.yml` from the host's `gh` login on
    /// each launch. When the host is logged out, the container's existing
    /// login (if any) is preserved.
    #[default]
    #[serde(rename = "sync")]
    Sync,
    /// Inject `GH_TOKEN` (and `GITHUB_TOKEN`) into the container's process
    /// env from the resolved operator-env layer. Any prior `hosts.yml`
    /// in role state is wiped so a stale file-based login cannot shadow
    /// the env token.
    #[serde(rename = "token")]
    Token,
    /// Wipe any forwarded `gh` state and never forward host auth.
    #[serde(rename = "ignore")]
    Ignore,
}

impl std::fmt::Display for GithubAuthMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sync => write!(f, "sync"),
            Self::Token => write!(f, "token"),
            Self::Ignore => write!(f, "ignore"),
        }
    }
}

impl std::str::FromStr for GithubAuthMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sync" => Ok(Self::Sync),
            "token" => Ok(Self::Token),
            "ignore" => Ok(Self::Ignore),
            other => Err(format!(
                "invalid github auth_forward mode {other:?}; expected one of: sync, token, ignore"
            )),
        }
    }
}

/// Operator-only `[github]` configuration block. Lives at the same
/// three layers as `[claude]` and `[codex]` (global, workspace,
/// workspace × role); role manifests cannot set or override it.
///
/// `env` is the operator env map for `token` mode and must contain
/// `GH_TOKEN`. Optionally also `GH_HOST` (for GHE) and
/// `GH_ENTERPRISE_TOKEN`. Values resolve through the same
/// `operator_env` dispatch as Claude / Codex auth env.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GithubAuthConfig {
    #[serde(default)]
    pub auth_forward: GithubAuthMode,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, crate::operator_env::EnvValue>,
}

/// Newtype around `AgentAuthConfig` that rejects `oauth_token` mode at parse time.
///
/// Codex does not support `AuthForwardMode::OAuthToken` — rejecting it at
/// deserialization time keeps the type system honest so downstream code
/// never has to handle the impossible combination.
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct CodexAuthConfig(pub(crate) AgentAuthConfig);

impl CodexAuthConfig {
    /// Construct, rejecting `OAuthToken`. The only public path to
    /// build the newtype, so the parse-time invariant survives
    /// post-deserialize.
    pub fn new(cfg: AgentAuthConfig) -> Result<Self, &'static str> {
        if cfg.auth_forward == AuthForwardMode::OAuthToken {
            return Err("auth_forward 'oauth_token' is not supported for codex");
        }
        Ok(Self(cfg))
    }
}

impl<'de> serde::Deserialize<'de> for CodexAuthConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let cfg = AgentAuthConfig::deserialize(deserializer)?;
        if cfg.auth_forward == AuthForwardMode::OAuthToken {
            return Err(serde::de::Error::custom(
                "auth_forward 'oauth_token' is not supported for codex; \
                 supported modes: sync, api_key, ignore",
            ));
        }
        Ok(Self(cfg))
    }
}

impl std::ops::Deref for CodexAuthConfig {
    type Target = AgentAuthConfig;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Newtype around `AgentAuthConfig` that rejects `oauth_token` at parse time.
///
/// Amp does not support `AuthForwardMode::OAuthToken` (the CLI authenticates
/// via an `AMP_API_KEY` or its own settings file); rejecting it at
/// deserialization time keeps the type system honest.
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct AmpAuthConfig(pub(crate) AgentAuthConfig);

impl AmpAuthConfig {
    /// Construct, rejecting `OAuthToken`. See [`CodexAuthConfig::new`].
    pub fn new(cfg: AgentAuthConfig) -> Result<Self, &'static str> {
        if cfg.auth_forward == AuthForwardMode::OAuthToken {
            return Err("auth_forward 'oauth_token' is not supported for amp");
        }
        Ok(Self(cfg))
    }
}

impl<'de> serde::Deserialize<'de> for AmpAuthConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let cfg = AgentAuthConfig::deserialize(deserializer)?;
        if cfg.auth_forward == AuthForwardMode::OAuthToken {
            return Err(serde::de::Error::custom(
                "auth_forward 'oauth_token' is not supported for amp; \
                 supported modes: sync, api_key, ignore",
            ));
        }
        Ok(Self(cfg))
    }
}

impl std::ops::Deref for AmpAuthConfig {
    type Target = AgentAuthConfig;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Newtype around `AgentAuthConfig` that rejects `oauth_token` at parse time.
///
/// Kimi does not support `AuthForwardMode::OAuthToken` — rejecting it at
/// deserialization time keeps the type system honest.
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct KimiAuthConfig(pub(crate) AgentAuthConfig);

impl KimiAuthConfig {
    /// Construct, rejecting `OAuthToken`. See [`CodexAuthConfig::new`].
    pub fn new(cfg: AgentAuthConfig) -> Result<Self, &'static str> {
        if cfg.auth_forward == AuthForwardMode::OAuthToken {
            return Err("auth_forward 'oauth_token' is not supported for kimi");
        }
        Ok(Self(cfg))
    }
}

impl<'de> serde::Deserialize<'de> for KimiAuthConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let cfg = AgentAuthConfig::deserialize(deserializer)?;
        if cfg.auth_forward == AuthForwardMode::OAuthToken {
            return Err(serde::de::Error::custom(
                "auth_forward 'oauth_token' is not supported for kimi; \
                 supported modes: sync, api_key, ignore",
            ));
        }
        Ok(Self(cfg))
    }
}

impl std::ops::Deref for KimiAuthConfig {
    type Target = AgentAuthConfig;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Newtype around `AgentAuthConfig` that rejects `oauth_token` at parse time.
///
/// Opencode does not support `AuthForwardMode::OAuthToken` — rejecting it at
/// deserialization time keeps the type system honest.
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct OpencodeAuthConfig(pub(crate) AgentAuthConfig);

impl OpencodeAuthConfig {
    /// Construct, rejecting `OAuthToken`. See [`CodexAuthConfig::new`].
    pub fn new(cfg: AgentAuthConfig) -> Result<Self, &'static str> {
        if cfg.auth_forward == AuthForwardMode::OAuthToken {
            return Err("auth_forward 'oauth_token' is not supported for opencode");
        }
        Ok(Self(cfg))
    }
}

impl<'de> serde::Deserialize<'de> for OpencodeAuthConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let cfg = AgentAuthConfig::deserialize(deserializer)?;
        if cfg.auth_forward == AuthForwardMode::OAuthToken {
            return Err(serde::de::Error::custom(
                "auth_forward 'oauth_token' is not supported for opencode; \
                 supported modes: sync, api_key, ignore",
            ));
        }
        Ok(Self(cfg))
    }
}

impl std::ops::Deref for OpencodeAuthConfig {
    type Target = AgentAuthConfig;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleSource {
    pub git: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub trusted: bool,
    /// Role-layer operator env map. Merged on top of the global
    /// `[env]` map when the role is launched. Values use the
    /// `operator_env` dispatch syntax.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, crate::operator_env::EnvValue>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerConfig {
    #[serde(default)]
    pub mounts: DockerMounts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "migrations::current_config_version", rename = "version")]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<AgentAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<CodexAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amp: Option<AmpAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kimi: Option<KimiAuthConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode: Option<OpencodeAuthConfig>,
    /// Global `[github]` block — bottom layer of the layered resolver
    /// (global → workspace → workspace × role). Operator-only; role
    /// manifests cannot set or override it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubAuthConfig>,
    /// Global operator env map — the bottom layer. Merged under
    /// per-role, per-workspace, and per-(workspace × role) layers.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, crate::operator_env::EnvValue>,
    #[serde(default)]
    pub roles: BTreeMap<String, RoleSource>,
    #[serde(default)]
    pub docker: DockerConfig,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub workspaces: BTreeMap<String, WorkspaceConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CURRENT_CONFIG_VERSION.to_string(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            github: None,
            env: BTreeMap::new(),
            roles: BTreeMap::new(),
            docker: DockerConfig::default(),
            workspaces: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use tempfile::tempdir;

    #[test]
    fn deserializes_scoped_docker_mounts() {
        let toml_str = r#"
[roles.agent-smith]
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
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/Users/donbeave/Projects/chainargos/big-monorepo"
default_role = "agent-smith"
allowed_roles = ["agent-smith", "chainargos/the-architect"]

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
        assert_eq!(workspace.default_role.as_deref(), Some("agent-smith"));
        assert_eq!(workspace.allowed_roles.len(), 2);
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
                isolation: crate::isolation::MountIsolation::Shared,
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
                        isolation: crate::isolation::MountIsolation::Shared,
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
[roles.agent-smith]
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
[roles.agent-smith]
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
    fn existing_config_without_claude_section_deserializes_with_defaults() {
        let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert!(
            config.claude.is_none(),
            "absent [claude] block must deserialize to None"
        );
        assert_eq!(
            crate::config::resolve_mode(&config, crate::agent::Agent::Claude, "", "agent-smith",),
            AuthForwardMode::Sync
        );
    }

    #[test]
    fn auth_forward_mode_from_str_accepts_oauth_token() {
        use std::str::FromStr;
        assert_eq!(
            AuthForwardMode::from_str("oauth_token").unwrap(),
            AuthForwardMode::OAuthToken
        );
    }

    #[test]
    fn auth_forward_mode_display_emits_oauth_token() {
        assert_eq!(AuthForwardMode::OAuthToken.to_string(), "oauth_token");
    }

    #[test]
    fn auth_forward_mode_deserializes_oauth_token() {
        let toml_str = r#"
[claude]
auth_forward = "oauth_token"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.claude.as_ref().unwrap().auth_forward,
            AuthForwardMode::OAuthToken
        );
    }

    #[test]
    fn parse_app_config_agent_auth_blocks() {
        let toml = r#"
[claude]
auth_forward = "sync"

[codex]
auth_forward = "api_key"

[amp]
auth_forward = "ignore"
"#;
        let cfg: AppConfig = toml::from_str(toml).unwrap();
        assert_eq!(
            cfg.claude.as_ref().unwrap().auth_forward,
            AuthForwardMode::Sync
        );
        assert_eq!(
            cfg.codex.as_ref().unwrap().auth_forward,
            AuthForwardMode::ApiKey
        );
        assert_eq!(
            cfg.amp.as_ref().unwrap().auth_forward,
            AuthForwardMode::Ignore
        );
    }

    #[test]
    fn parse_app_config_no_agent_blocks() {
        let toml = "";
        let cfg: AppConfig = toml::from_str(toml).unwrap();
        assert!(
            cfg.claude.is_none(),
            "claude must be None when [claude] absent"
        );
        assert!(
            cfg.codex.is_none(),
            "codex must be None when [codex] absent"
        );
        assert!(cfg.amp.is_none(), "amp must be None when [amp] absent");
    }

    #[test]
    fn reject_codex_oauth_token_global() {
        let toml = r#"
[codex]
auth_forward = "oauth_token"
"#;
        let err = toml::from_str::<AppConfig>(toml).expect_err("must reject");
        let msg = err.to_string();
        assert!(
            msg.contains("not supported for codex"),
            "expected codex-rejection message, got: {msg}"
        );
    }

    #[test]
    fn reject_amp_oauth_token_global() {
        let toml = r#"
[amp]
auth_forward = "oauth_token"
"#;
        let err = toml::from_str::<AppConfig>(toml).expect_err("must reject");
        let msg = err.to_string();
        assert!(
            msg.contains("not supported for amp"),
            "expected amp-rejection message, got: {msg}"
        );
    }

    #[test]
    fn auth_forward_mode_from_str_error_lists_oauth_token() {
        use std::str::FromStr;
        let err = AuthForwardMode::from_str("nope").unwrap_err();
        assert!(
            err.contains("oauth_token"),
            "error message should advertise the oauth_token mode; got: {err}"
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
                        isolation: crate::isolation::MountIsolation::Shared,
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
                        isolation: crate::isolation::MountIsolation::Shared,
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
                        isolation: crate::isolation::MountIsolation::Shared,
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
                        isolation: crate::isolation::MountIsolation::Shared,
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
                            isolation: crate::isolation::MountIsolation::Shared,
                        },
                        MountConfig {
                            src: "/a/c".into(),
                            dst: "/a/c".into(),
                            readonly: false,
                            isolation: crate::isolation::MountIsolation::Shared,
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
                        isolation: crate::isolation::MountIsolation::Shared,
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
        // (e.g., adding an allowed role) should be blocked by the post-check.
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
                        isolation: crate::isolation::MountIsolation::Shared,
                    },
                    MountConfig {
                        src: "/a/b".into(),
                        dst: "/a/b".into(),
                        readonly: false,
                        isolation: crate::isolation::MountIsolation::Shared,
                    },
                ],
                ..Default::default()
            },
        );

        let err = config
            .edit_workspace(
                "legacy",
                WorkspaceEdit {
                    allowed_roles_to_add: vec!["agent-x".into()],
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
                            isolation: crate::isolation::MountIsolation::Shared,
                        },
                        MountConfig {
                            src: "/a/b".into(),
                            dst: "/a/b".into(),
                            readonly: false,
                            isolation: crate::isolation::MountIsolation::Shared,
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
                            isolation: crate::isolation::MountIsolation::Shared,
                        },
                        MountConfig {
                            src: "/a/b".into(),
                            dst: "/a/b".into(),
                            readonly: true,
                            isolation: crate::isolation::MountIsolation::Shared,
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
                        isolation: crate::isolation::MountIsolation::Shared,
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
    fn auth_forward_mode_display_emits_canonical_names() {
        assert_eq!(AuthForwardMode::Sync.to_string(), "sync");
        assert_eq!(AuthForwardMode::Ignore.to_string(), "ignore");
        assert_eq!(AuthForwardMode::ApiKey.to_string(), "api_key");
        assert_eq!(AuthForwardMode::OAuthToken.to_string(), "oauth_token");
    }

    #[test]
    fn parse_agent_auth_config_sync() {
        let toml = r#"auth_forward = "sync""#;
        let cfg: AgentAuthConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.auth_forward, AuthForwardMode::Sync);
    }

    #[test]
    fn parse_agent_auth_config_api_key() {
        let toml = r#"auth_forward = "api_key""#;
        let cfg: AgentAuthConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.auth_forward, AuthForwardMode::ApiKey);
    }

    #[test]
    fn parse_agent_auth_config_oauth_token() {
        let toml = r#"auth_forward = "oauth_token""#;
        let cfg: AgentAuthConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.auth_forward, AuthForwardMode::OAuthToken);
    }

    #[test]
    fn parse_agent_auth_config_ignore() {
        let toml = r#"auth_forward = "ignore""#;
        let cfg: AgentAuthConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.auth_forward, AuthForwardMode::Ignore);
    }

    #[test]
    fn agent_auth_config_serializes_canonical_names() {
        for (mode, expected) in [
            (AuthForwardMode::Sync, "sync"),
            (AuthForwardMode::ApiKey, "api_key"),
            (AuthForwardMode::OAuthToken, "oauth_token"),
            (AuthForwardMode::Ignore, "ignore"),
        ] {
            let cfg = AgentAuthConfig { auth_forward: mode };
            let s = toml::to_string(&cfg).expect("serialize must succeed");
            assert!(
                s.contains(&format!("auth_forward = \"{expected}\"")),
                "mode {mode:?} must serialize as auth_forward = \"{expected}\", got: {s}"
            );
        }
    }

    #[test]
    fn agent_auth_config_rejects_unknown_field() {
        let toml = "auth_forward = \"sync\"\nbogus = true";
        let err = toml::from_str::<AgentAuthConfig>(toml).expect_err("must reject");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field `bogus`") || msg.contains("unknown field \"bogus\""),
            "expected unknown-field error, got: {msg}"
        );
    }

    /// `oauth_token` is no longer a field on `AgentAuthConfig` — credentials
    /// live in the `[env]` block. Configs that still carry the old field
    /// are rejected by `deny_unknown_fields`.
    #[test]
    fn agent_auth_config_rejects_legacy_oauth_token_field() {
        let toml = "auth_forward = \"oauth_token\"\noauth_token = \"sk-ant-oat01-literal\"";
        let err = toml::from_str::<AgentAuthConfig>(toml).expect_err("must reject");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field"),
            "expected unknown-field rejection, got: {msg}"
        );
    }

    /// `oauth_token` in `[codex]` is rejected as an unknown field.
    #[test]
    fn codex_auth_config_rejects_oauth_token_field() {
        let toml = "auth_forward = \"api_key\"\noauth_token = \"doesnt-belong\"";
        let err = toml::from_str::<CodexAuthConfig>(toml).expect_err("must reject");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field"),
            "expected unknown-field rejection, got: {msg}"
        );
    }

    /// Same rejection through the top-level `AppConfig` parse path.
    #[test]
    fn reject_codex_oauth_token_field_at_app_config_layer() {
        let toml = "[codex]\nauth_forward = \"api_key\"\noauth_token = \"wrong-place\"";
        let err = toml::from_str::<AppConfig>(toml).expect_err("must reject");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field"),
            "expected unknown-field rejection at AppConfig layer, got: {msg}"
        );
    }

    #[test]
    fn agent_auth_config_serializes_without_extraneous_fields() {
        let cfg = AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
        };
        let s = toml::to_string(&cfg).unwrap();
        assert!(
            !s.contains("oauth_token"),
            "serialized config must not contain oauth_token, got:\n{s}"
        );
    }

    #[test]
    fn reject_legacy_role_claude_block() {
        let toml = r#"
[roles.smith]
git = "git@example.com:smith.git"
trusted = true

[roles.smith.claude]
auth_forward = "ignore"
"#;
        let err = toml::from_str::<AppConfig>(toml).expect_err("must reject legacy block");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field `claude`") || msg.contains("unknown field \"claude\""),
            "expected unknown-field error for legacy [roles.X.claude] block, got: {msg}"
        );
    }

    // ── GitHub auth schema ──────────────────────────────────────────────

    #[test]
    fn parse_app_config_with_global_github_block() {
        let toml = r#"
[github]
auth_forward = "sync"
"#;
        let cfg: AppConfig = toml::from_str(toml).unwrap();
        let g = cfg.github.as_ref().expect("[github] must parse");
        assert_eq!(g.auth_forward, super::GithubAuthMode::Sync);
        assert!(g.env.is_empty());
    }

    #[test]
    fn parse_app_config_with_github_token_and_env() {
        let toml = r#"
[github]
auth_forward = "token"

[github.env]
GH_TOKEN = "$GH_TOKEN"
GH_HOST = "ghe.acme.com"
"#;
        let cfg: AppConfig = toml::from_str(toml).unwrap();
        let g = cfg.github.as_ref().unwrap();
        assert_eq!(g.auth_forward, super::GithubAuthMode::Token);
        assert!(g.env.contains_key("GH_TOKEN"));
        assert!(g.env.contains_key("GH_HOST"));
    }

    #[test]
    fn parse_workspace_github_block() {
        let toml = r#"
[roles.smith]
git = "https://github.com/example/smith.git"

[workspaces.acme]
workdir = "/workspace/proj"

[[workspaces.acme.mounts]]
src = "/tmp/proj"
dst = "/workspace/proj"

[workspaces.acme.github]
auth_forward = "token"

[workspaces.acme.github.env]
GH_TOKEN = "op://Work/ACME/gh-pat"
"#;
        let cfg: AppConfig = toml::from_str(toml).unwrap();
        let ws = cfg.workspaces.get("acme").unwrap();
        let g = ws.github.as_ref().unwrap();
        assert_eq!(g.auth_forward, super::GithubAuthMode::Token);
        assert!(g.env.contains_key("GH_TOKEN"));
    }

    #[test]
    fn parse_workspace_role_override_github_block() {
        let toml = r#"
[roles.smith]
git = "https://github.com/example/smith.git"

[workspaces.acme]
workdir = "/workspace/proj"

[[workspaces.acme.mounts]]
src = "/tmp/proj"
dst = "/workspace/proj"

[workspaces.acme.roles.smith.github]
auth_forward = "ignore"
"#;
        let cfg: AppConfig = toml::from_str(toml).unwrap();
        let ov = cfg
            .workspaces
            .get("acme")
            .and_then(|ws| ws.roles.get("smith"))
            .expect("override must exist");
        let g = ov.github.as_ref().unwrap();
        assert_eq!(g.auth_forward, super::GithubAuthMode::Ignore);
    }

    #[test]
    fn github_auth_mode_default_is_sync() {
        assert_eq!(
            super::GithubAuthMode::default(),
            super::GithubAuthMode::Sync
        );
    }

    #[test]
    fn github_auth_mode_from_str_round_trips() {
        use std::str::FromStr;
        assert_eq!(
            super::GithubAuthMode::from_str("sync").unwrap(),
            super::GithubAuthMode::Sync
        );
        assert_eq!(
            super::GithubAuthMode::from_str("token").unwrap(),
            super::GithubAuthMode::Token
        );
        assert_eq!(
            super::GithubAuthMode::from_str("ignore").unwrap(),
            super::GithubAuthMode::Ignore
        );
        assert!(super::GithubAuthMode::from_str("api_key").is_err());
        assert!(super::GithubAuthMode::from_str("oauth_token").is_err());
        assert!(super::GithubAuthMode::from_str("nope").is_err());
    }

    #[test]
    fn github_auth_mode_display_emits_canonical_names() {
        assert_eq!(super::GithubAuthMode::Sync.to_string(), "sync");
        assert_eq!(super::GithubAuthMode::Token.to_string(), "token");
        assert_eq!(super::GithubAuthMode::Ignore.to_string(), "ignore");
    }

    #[test]
    fn github_auth_config_rejects_unknown_field() {
        let toml = r#"
auth_forward = "sync"
bogus = true
"#;
        let err =
            toml::from_str::<super::GithubAuthConfig>(toml).expect_err("unknown field must reject");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field `bogus`") || msg.contains("unknown field \"bogus\""),
            "expected unknown-field error, got: {msg}"
        );
    }

    #[test]
    fn resolve_github_mode_layered_precedence() {
        use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
        let mut cfg = AppConfig::default();
        // Default — Sync
        assert_eq!(
            super::resolve_github_mode(&cfg, "proj", "smith"),
            super::GithubAuthMode::Sync
        );
        // Global only
        cfg.github = Some(super::GithubAuthConfig {
            auth_forward: super::GithubAuthMode::Ignore,
            env: BTreeMap::new(),
        });
        assert_eq!(
            super::resolve_github_mode(&cfg, "proj", "smith"),
            super::GithubAuthMode::Ignore
        );
        // Workspace overrides global
        let ws = WorkspaceConfig {
            workdir: "/x".into(),
            github: Some(super::GithubAuthConfig {
                auth_forward: super::GithubAuthMode::Token,
                env: BTreeMap::new(),
            }),
            ..Default::default()
        };
        cfg.workspaces.insert("proj".into(), ws);
        assert_eq!(
            super::resolve_github_mode(&cfg, "proj", "smith"),
            super::GithubAuthMode::Token
        );
        // Role override wins
        let ov = WorkspaceRoleOverride {
            github: Some(super::GithubAuthConfig {
                auth_forward: super::GithubAuthMode::Sync,
                env: BTreeMap::new(),
            }),
            ..WorkspaceRoleOverride::default()
        };
        cfg.workspaces
            .get_mut("proj")
            .unwrap()
            .roles
            .insert("smith".into(), ov);
        assert_eq!(
            super::resolve_github_mode(&cfg, "proj", "smith"),
            super::GithubAuthMode::Sync
        );
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
        assert_eq!(
            config
                .env
                .get("OPERATOR_GLOBAL")
                .unwrap()
                .as_persisted_str(),
            "literal"
        );
        assert_eq!(
            config
                .env
                .get("OPERATOR_SECRET")
                .unwrap()
                .as_persisted_str(),
            "op://Personal/api/token"
        );
        assert_eq!(
            config.env.get("OPERATOR_HOST").unwrap().as_persisted_str(),
            "$HOME_VAR"
        );
    }

    #[test]
    fn deserializes_per_agent_env_map() {
        let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[roles.agent-smith.env]
AGENT_TOKEN = "op://Shared/smith/token"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let role = config.roles.get("agent-smith").unwrap();
        assert_eq!(
            role.env.get("AGENT_TOKEN").unwrap().as_persisted_str(),
            "op://Shared/smith/token"
        );
    }

    #[test]
    fn deserializes_per_workspace_env_map() {
        let toml_str = r#"
[roles.agent-smith]
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
        assert_eq!(
            ws.env.get("WORKSPACE_VAR").unwrap().as_persisted_str(),
            "literal"
        );
    }

    #[test]
    fn deserializes_workspace_agent_override_env() {
        let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.roles.agent-smith.env]
PER_WORKSPACE_PER_AGENT = "specific"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let ws = config.workspaces.get("big-monorepo").unwrap();
        let override_ = ws.roles.get("agent-smith").unwrap();
        assert_eq!(
            override_
                .env
                .get("PER_WORKSPACE_PER_AGENT")
                .unwrap()
                .as_persisted_str(),
            "specific"
        );
    }

    #[test]
    fn env_maps_default_to_empty_when_omitted() {
        let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert!(config.env.is_empty());
        assert!(config.roles.get("agent-smith").unwrap().env.is_empty());
    }

    #[test]
    fn deserializes_agent_with_slash_in_name_using_quoted_keys() {
        // The spec calls out `[roles."chainargos/agent-jones".env]`
        // and `[workspaces.<ws>.roles."chainargos/agent-jones".env]`
        // as the TOML shape for third-party role selectors that
        // include a `/`. Standard TOML quoted keys suffice — this
        // test locks in that shape so a future refactor does not
        // accidentally require un-quoted identifiers.
        let toml_str = r#"
[roles."chainargos/agent-jones"]
git = "https://github.com/chainargos/jackin-agent-jones.git"

[roles."chainargos/agent-jones".env]
DATABASE_URL = "op://Work/agent-jones/db"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.roles."chainargos/agent-jones".env]
OPENAI_API_KEY = "op://Work/big-monorepo/OpenAI"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let role = config.roles.get("chainargos/agent-jones").unwrap();
        assert_eq!(
            role.env.get("DATABASE_URL").unwrap().as_persisted_str(),
            "op://Work/agent-jones/db"
        );
        let ws = config.workspaces.get("big-monorepo").unwrap();
        let override_ = ws.roles.get("chainargos/agent-jones").unwrap();
        assert_eq!(
            override_
                .env
                .get("OPENAI_API_KEY")
                .unwrap()
                .as_persisted_str(),
            "op://Work/big-monorepo/OpenAI"
        );
    }
}
