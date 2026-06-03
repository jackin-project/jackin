//! jackin' configuration schema and public re-exports for the `config`
//! subsystem.
//!
//! The shared schema types (`WorkspaceConfig`, `MountConfig`, `AppConfig`, etc.)
//! are defined in `jackin-config` and re-exported here so that
//! `use crate::config::Foo` works. Behavior (TOML read/write, migrations,
//! workspace resolution) lives in the sub-modules below.

pub mod editor;
pub(crate) mod migrations;
mod mounts;
pub(crate) mod persist;
mod roles;
#[cfg(test)]
mod tests;
mod workspaces;

pub use crate::runtime::drift::{DriftDetection, detect_workspace_edit_drift};
pub use editor::{ConfigEditor, EnvScope};
pub use migrations::{migrate_config_file_if_needed, migrate_workspace_file_if_needed};
pub(crate) use mounts::MountEntry;
pub use mounts::{DockerMounts, GlobalMountRow, WorkspaceGlobalMountRows};
pub use roles::{
    build_github_env_layers, resolve_github_mode, resolve_mode, resolve_mode_with_trace,
};

/// Re-exported from `jackin-core`.
pub use jackin_core::AuthForwardMode;

/// Re-exported from `jackin-config` — workspace schema lives there.
/// `AppConfig` stays in this crate until `JackinPaths` is also extractable.
pub use jackin_config::{
    AgentAuthConfig, AmpAuthConfig, CURRENT_CONFIG_VERSION, CURRENT_WORKSPACE_VERSION,
    CodexAuthConfig, GitConfig, GithubAuthConfig, GithubAuthMode, GlobalMountConfig,
    KeepAwakeConfig, KimiAuthConfig, MountConfig, MountIsolation, OpencodeAuthConfig, RoleSource,
    WorkspaceConfig, WorkspaceRoleOverride,
};

/// `WorkspaceEdit` lives in the binary crate (mutation helper, not schema).
pub use crate::workspace::{WorkspaceEdit, validate_workspace_config};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The top-level operator configuration (`~/.config/jackin/config.toml`).
///
/// Schema types (`WorkspaceConfig`, `MountConfig`, etc.) now come from
/// `jackin-config`; this struct assembles them. Behavior (load/save) lives
/// in `config/persist`.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubAuthConfig>,
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

/// Docker/mount configuration block.
///
/// Note: `DockerMounts` (the complex nested structure used by the TUI)
/// stays in `config/mounts/` while `DockerConfig` is a thin wrapper.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerConfig {
    #[serde(default)]
    pub mounts: DockerMounts,
}
