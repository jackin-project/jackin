// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub mod mounts;
pub mod paths;
pub(crate) mod planner;
pub mod resolve;
pub mod sensitive;

pub use jackin_config::{
    validate_isolation_layout, validate_mount_paths, validate_mount_specs, validate_mounts,
    validate_workspace_config,
};
pub use mounts::{parse_mount_spec, parse_mount_spec_resolved};
pub use paths::{expand_tilde, resolve_path};
pub use planner::{CollapseError, CollapsePlan, Removal, plan_collapse};
pub use resolve::{
    LoadWorkspaceInput, ResolvedWorkspace, current_dir_workspace, resolve_load_workspace,
    saved_workspace_match_depth,
};
pub use sensitive::{
    SensitiveMount, confirm_sensitive_mounts, find_sensitive_mounts, resolved_sensitive_mounts,
};

pub use jackin_config::{
    DirtyExitPolicy, KeepAwakeConfig, MountConfig, MountIsolation, WorkspaceConfig, WorkspaceEdit,
    WorkspaceRoleOverride,
};
