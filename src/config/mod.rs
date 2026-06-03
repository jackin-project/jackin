//! Configuration re-exports — `AppConfig`, `ConfigEditor`, and all config
//! types are now in `jackin-config`. This module is a thin shim.

pub mod editor;
pub(crate) mod migrations;
pub mod mounts;
pub(crate) mod persist;
pub(crate) mod roles;
#[cfg(test)]
mod tests;
pub(crate) mod workspaces;

// Note: DriftDetection and detect_workspace_edit_drift are NOT re-exported
// here. They live in runtime::drift and must be imported from there directly.
pub use jackin_config::{
    AppConfig, ConfigEditor, EnvScope, GlobalMountRow, WorkspaceGlobalMountRows,
    AgentAuthConfig, AmpAuthConfig, CURRENT_CONFIG_VERSION, CURRENT_WORKSPACE_VERSION,
    CodexAuthConfig, DockerConfig, DockerMounts, GitConfig, GithubAuthConfig, GithubAuthMode,
    GlobalMountConfig, KeepAwakeConfig, KimiAuthConfig, MountConfig, MountEntry, MountIsolation,
    OpencodeAuthConfig, RoleSource, WorkspaceConfig, WorkspaceEdit, WorkspaceRoleOverride,
    build_github_env_layers, resolve_github_mode, resolve_mode, resolve_mode_with_trace,
    AuthForwardMode,
};
pub use jackin_config::migrations::{
    migrate_config_file_if_needed, migrate_workspace_file_if_needed,
};
pub use crate::workspace::validate_workspace_config;

#[cfg(test)]
pub(crate) use std::collections::BTreeMap;
