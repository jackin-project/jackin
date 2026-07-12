//! jackin-config: operator config load, migrate, validate, and persist.
//!
//! **Architecture Invariant:** T1.
//! Entry point: [`AppConfig`] — loaded operator configuration.

#![deny(
    clippy::string_slice,
    clippy::indexing_slicing,
    clippy::get_unwrap,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn,
    clippy::unchecked_time_subtraction
)]
#![deny(missing_docs)]
// get_unwrap has no clippy.toml allow-in-tests valve; keep production denied.
#![cfg_attr(
    test,
    allow(
        clippy::get_unwrap,
        reason = "no clippy.toml allow-in-tests valve; keep production denied"
    )
)]

pub mod app_config;
pub mod auth;
pub mod editor;
mod error;
pub mod migrations;
pub mod mounts;
pub mod paths;
pub mod persist;
pub mod planner;
pub mod resolve;
pub mod schema;
pub mod sensitive;
pub mod validation;
pub mod versions;

pub use error::ConfigError;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use app_config::AppConfig;
pub use app_config::mounts::{GlobalMountRow, WorkspaceGlobalMountRows};
pub use app_config::persist::{
    config_needs_split_migration, load_split_config, validate_reserved_env_names,
};
pub use app_config::roles::{
    BUILTIN_ROLES, build_github_env_layers, resolve_github_mode, resolve_mode,
    resolve_mode_with_trace, resolve_sync_source_dir,
};
pub use auth::{AgentAuthConfig, GithubAuthConfig, GithubAuthMode};
pub use editor::{ConfigEditor, EnvScope};
pub use jackin_core::{AuthForwardMode, EnvValue, FieldTarget, MountIsolation, OpRef};
pub use migrations::{
    Channel, Migration, MigrationStep, SchemaVersion, apply_migrations, doc_version,
    migrate_config_file_if_needed, migrate_file_if_needed, migrate_workspace_file_if_needed,
    migrate_workspace_op_account_to_refs, noop_migration, parse_registry_version, parse_version,
    set_doc_version,
};
pub use mounts::{parse_mount_spec, parse_mount_spec_resolved};
pub use paths::{expand_tilde, resolve_path};
pub use persist::{atomic_write, validate_workspace_file_stem};
pub use planner::{
    CollapseError, CollapsePlan, Removal, WorkspaceCreatePlan, WorkspaceEditPlan,
    apply_isolation_overrides, plan_collapse, plan_create, plan_edit,
};
pub use resolve::{
    LoadWorkspaceInput, current_dir_workspace, find_saved_workspace_for_cwd,
    resolve_load_workspace, saved_workspace_match_depth,
};
pub use schema::{
    DirtyExitPolicy, DockerConfig, DockerMounts, GitConfig, GlobalMountConfig, KeepAwakeConfig,
    MountConfig, MountEntry, ResolvedWorkspace, RoleSource, RuntimeConfig, TelemetryConfig,
    TelemetryLevelConfig, WorkspaceConfig, WorkspaceDockerConfig, WorkspaceEdit,
    WorkspaceRoleOverride, WorkspaceRuntimeConfig, validate_mount_paths, validate_mount_specs,
    validate_mounts,
};
pub use sensitive::{SensitiveMount, find_sensitive_mounts};
pub use validation::{validate_isolation_layout, validate_workspace_config};
pub use versions::{
    CURRENT_CONFIG_VERSION, CURRENT_WORKSPACE_VERSION, LEGACY_VERSION, current_config_version,
    current_workspace_version,
};
