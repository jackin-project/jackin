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

/// Re-exported from `jackin-config` — the canonical definitions live there.
pub use jackin_config::{
    AgentAuthConfig, AmpAuthConfig, CodexAuthConfig, GithubAuthConfig, GithubAuthMode,
    KimiAuthConfig, OpencodeAuthConfig,
};

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
