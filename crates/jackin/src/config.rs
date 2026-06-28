//! Configuration re-exports — `AppConfig`, `ConfigEditor`, and all config
//! types are now in `jackin-config`. This module is a thin shim.

// Note: DriftDetection and detect_workspace_edit_drift are NOT re-exported
// here. They live in runtime::drift and must be imported from there directly.
pub use crate::workspace::validate_workspace_config;
pub use jackin_config::{
    AgentAuthConfig, AppConfig, AuthForwardMode, CURRENT_CONFIG_VERSION, CURRENT_WORKSPACE_VERSION,
    ConfigEditor, DirtyExitPolicy, DockerConfig, DockerMounts, EnvScope, GitConfig,
    GithubAuthConfig, GithubAuthMode, GlobalMountConfig, GlobalMountRow, KeepAwakeConfig,
    MountConfig, MountEntry, MountIsolation, RoleSource, WorkspaceConfig, WorkspaceEdit,
    WorkspaceGlobalMountRows, WorkspaceRoleOverride, build_github_env_layers, resolve_github_mode,
    resolve_mode, resolve_mode_with_trace, resolve_sync_source_dir,
};
pub use jackin_config::{migrate_config_file_if_needed, migrate_workspace_file_if_needed};
