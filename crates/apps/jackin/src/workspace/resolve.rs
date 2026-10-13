// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace resolution — re-exported from `jackin-config`.

pub use jackin_config::{
    LoadWorkspaceInput, ResolvedWorkspace, current_dir_workspace, resolve_load_workspace,
    saved_workspace_match_depth,
};
