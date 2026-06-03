//! jackin' configuration schema and public re-exports for the `config`
//! subsystem.
//!
//! Re-exports the types callers depend on — `AppConfig`, `AuthForwardMode`,
//! `ConfigEditor`, migration helpers, and mount helpers — so that
//! `use crate::config::Foo` works without navigating sub-modules.
//!
//! Schema versioning lives in `config/migrations.rs`. Editor behavior (TOML
//! reads and writes) lives in `config/editor.rs`. Workspace types and
//! resolution logic live in `workspace/`.

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
pub use roles::{
    build_github_env_layers, resolve_github_mode, resolve_mode, resolve_mode_with_trace,
};
pub use workspaces::{DriftDetection, detect_workspace_edit_drift};

/// Serde helper: `skip_serializing_if` requires `fn(&T) -> bool`.
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(v: &bool) -> bool {
    !*v
}

/// Re-exported from `jackin-core` — the canonical definition lives there.
pub use jackin_core::AuthForwardMode;

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

/// Generate an `AgentAuthConfig` newtype that rejects `OAuthToken` at both
/// parse time (serde) and construction time (`new`). Used for agents that do
/// not support OAuth token forwarding: Codex, Amp, Kimi, `OpenCode`.
macro_rules! agent_auth_config_no_oauth {
    ($name:ident, $agent:literal) => {
        #[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
        pub struct $name(pub(crate) AgentAuthConfig);

        impl $name {
            /// Construct, rejecting `OAuthToken`. The only public path to build
            /// the newtype outside of serde, so the invariant holds end-to-end.
            pub fn new(cfg: AgentAuthConfig) -> Result<Self, &'static str> {
                if cfg.auth_forward == AuthForwardMode::OAuthToken {
                    return Err(concat!(
                        "auth_forward 'oauth_token' is not supported for ",
                        $agent
                    ));
                }
                Ok(Self(cfg))
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let cfg = AgentAuthConfig::deserialize(deserializer)?;
                if cfg.auth_forward == AuthForwardMode::OAuthToken {
                    return Err(serde::de::Error::custom(concat!(
                        "auth_forward 'oauth_token' is not supported for ",
                        $agent,
                        "; supported modes: sync, api_key, ignore"
                    )));
                }
                Ok(Self(cfg))
            }
        }

        impl std::ops::Deref for $name {
            type Target = AgentAuthConfig;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
}

agent_auth_config_no_oauth!(CodexAuthConfig, "codex");
agent_auth_config_no_oauth!(AmpAuthConfig, "amp");
agent_auth_config_no_oauth!(KimiAuthConfig, "kimi");
agent_auth_config_no_oauth!(OpencodeAuthConfig, "opencode");

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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitConfig {
    #[serde(default, skip_serializing_if = "is_false")]
    pub coauthor_trailer: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dco: bool,
}

impl GitConfig {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }
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
    #[serde(default, skip_serializing_if = "GitConfig::is_default")]
    pub git: GitConfig,
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
            git: GitConfig::default(),
            workspaces: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests;
