// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace save preview types and builders.

use super::{
    SettingsEnvPreview, WorkspaceAuthChange, WorkspaceMountDiff, workspace_auth_changes,
    workspace_env_preview, workspace_mount_diffs_preview, workspace_save_lines,
};

use ratatui::text::Line;

use crate::tui::screens::editor::model::{EditorMode, EditorState};

/// Toggleable workspace settings captured at edit time. Bundled so the parent
/// `WorkspaceSavePreview` keeps the `struct_excessive_bools` clippy gate quiet
/// while preserving the original/pending diff display surface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkspaceToggleSet {
    pub keep_awake: bool,
    pub git_pull: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSavePreview {
    pub mode: WorkspaceSaveMode,
    pub original_workdir: Option<String>,
    pub pending_workdir: String,
    pub mount_diffs: Vec<WorkspaceMountDiff>,
    pub auth_changes: Vec<WorkspaceAuthChange>,
    pub original_allowed_roles: Vec<String>,
    pub pending_allowed_roles: Vec<String>,
    pub role_count: usize,
    pub original_default_role: Option<String>,
    pub pending_default_role: Option<String>,
    pub original_toggles: WorkspaceToggleSet,
    pub pending_toggles: WorkspaceToggleSet,
    pub env_original: SettingsEnvPreview,
    pub env_pending: SettingsEnvPreview,
    pub collapse_lines: Vec<Line<'static>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSaveMode {
    Create {
        name: String,
    },
    Edit {
        original_name: String,
        display_name: String,
        pending_name: Option<String>,
    },
}

#[must_use]
pub fn workspace_create_display_name(pending_name: Option<&str>) -> String {
    pending_name.unwrap_or("(unnamed)").to_owned()
}

#[must_use]
pub fn workspace_save_preview<
    Modal,
    SaveFlow,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    editor: &EditorState<
        crate::mount_info_cache::MountInfoCache,
        Modal,
        SaveFlow,
        jackin_config::EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
    collapse_lines: &[Line<'static>],
) -> WorkspaceSavePreview {
    let mode = match &editor.mode {
        EditorMode::Create => WorkspaceSaveMode::Create {
            name: workspace_create_display_name(editor.pending_name.as_deref()),
        },
        EditorMode::Edit { name } => WorkspaceSaveMode::Edit {
            original_name: name.clone(),
            display_name: editor.pending_name.clone().unwrap_or_else(|| name.clone()),
            pending_name: editor.pending_name.clone(),
        },
    };

    let workspace_name = match &editor.mode {
        EditorMode::Edit { name } => name.as_str(),
        EditorMode::Create => editor.pending_name.as_deref().unwrap_or("(new workspace)"),
    };

    WorkspaceSavePreview {
        mode,
        original_workdir: matches!(editor.mode, EditorMode::Edit { .. })
            .then(|| jackin_core::shorten_home(&editor.original.workdir)),
        pending_workdir: jackin_core::shorten_home(&editor.pending.workdir),
        mount_diffs: workspace_mount_diffs_preview(editor),
        auth_changes: workspace_auth_changes(
            config,
            workspace_name,
            &editor.original,
            &editor.pending,
        ),
        original_allowed_roles: editor.original.allowed_roles.clone(),
        pending_allowed_roles: editor.pending.allowed_roles.clone(),
        role_count: config.roles.len(),
        original_default_role: editor.original.default_role.clone(),
        pending_default_role: editor.pending.default_role.clone(),
        original_toggles: WorkspaceToggleSet {
            keep_awake: editor.original.keep_awake.enabled,
            git_pull: editor.original.git_pull_on_entry,
        },
        pending_toggles: WorkspaceToggleSet {
            keep_awake: editor.pending.keep_awake.enabled,
            git_pull: editor.pending.git_pull_on_entry,
        },
        env_original: workspace_env_preview(&editor.original),
        env_pending: workspace_env_preview(&editor.pending),
        collapse_lines: collapse_lines.to_vec(),
    }
}

#[must_use]
pub fn build_workspace_save_lines<
    Modal,
    SaveFlow,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    editor: &EditorState<
        crate::mount_info_cache::MountInfoCache,
        Modal,
        SaveFlow,
        jackin_config::EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
    collapse_lines: &[Line<'static>],
) -> Vec<Line<'static>> {
    workspace_save_lines(&workspace_save_preview(editor, config, collapse_lines))
}
