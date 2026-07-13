// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace configuration types and resolution.
//!
//! Schema types (`WorkspaceConfig`, `MountConfig`, etc.) are defined in
//! `jackin-config` and re-exported here. This module owns:
//! - Path helpers, planner, mount validation, workspace resolution
//! - `WorkspaceEdit` mutation helper (not a schema type)
//! - `validate_workspace_config` (behavior, not schema)
//! - The orphan-rule impls for jackin-console traits on jackin-config types
//!
//! Not responsible for: reading or writing workspace files (`config/editor.rs`
//! via `ConfigEditor`), or container mount materialization
//! (`isolation/materialize.rs`).

pub mod mounts;
pub mod paths;
pub(crate) mod planner;
pub mod resolve;
pub mod sensitive;
pub mod token_setup;

pub use jackin_config::{
    validate_isolation_layout, validate_mount_paths, validate_mount_specs, validate_mounts,
    validate_workspace_config,
};
#[cfg(test)]
pub(crate) use mounts::covers;
pub use mounts::{parse_mount_spec, parse_mount_spec_resolved};
pub use paths::{expand_tilde, resolve_path};
pub use planner::{CollapseError, CollapsePlan, Removal, plan_collapse};
pub use resolve::{
    LoadWorkspaceInput, ResolvedWorkspace, current_dir_workspace, resolve_load_workspace,
    saved_workspace_match_depth,
};
pub use sensitive::{SensitiveMount, confirm_sensitive_mounts, find_sensitive_mounts};

/// Re-exported schema types from `jackin-config`.
pub use jackin_config::{
    DirtyExitPolicy, KeepAwakeConfig, MountConfig, MountIsolation, WorkspaceConfig, WorkspaceEdit,
    WorkspaceRoleOverride,
};

#[cfg(test)]
mod tests;
