use std::collections::BTreeSet;

use ratatui::text::Line;

use crate::tui::components::editor_rows::{AuthSourceFolderDisplay, AuthSourceFolderKind};
use crate::tui::screens::editor::model::{EditorMode, EditorState};
use crate::tui::{
    auth::{AuthKind, AuthMode, auth_mode_supports_source_folder},
    auth_config::{
        auth_kind_agent, editor_source_folder_display, env_display_map_without_auth_credentials,
        panel_auth_source_value, resolve_panel_mode, role_auth_mode_and_credential,
    },
};

use super::settings::SettingsEnvPreview;
use super::workspace_lines::workspace_save_lines;

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
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    editor: &EditorState<
        jackin_config::WorkspaceConfig,
        crate::mount_info_cache::MountInfoCache,
        Modal,
        SaveFlow,
        jackin_config::EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
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
            .then(|| jackin_tui::shorten_home(&editor.original.workdir)),
        pending_workdir: jackin_tui::shorten_home(&editor.pending.workdir),
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
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    editor: &EditorState<
        jackin_config::WorkspaceConfig,
        crate::mount_info_cache::MountInfoCache,
        Modal,
        SaveFlow,
        jackin_config::EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
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

fn workspace_mount_diffs_preview<
    Modal,
    SaveFlow,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    editor: &EditorState<
        jackin_config::WorkspaceConfig,
        crate::mount_info_cache::MountInfoCache,
        Modal,
        SaveFlow,
        jackin_config::EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
) -> Vec<WorkspaceMountDiff> {
    match editor.mode {
        EditorMode::Create => editor
            .pending
            .mounts
            .iter()
            .map(|mount| {
                WorkspaceMountDiff::Added(workspace_mount_preview_row(
                    mount,
                    &editor.mount_info_cache,
                ))
            })
            .collect(),
        EditorMode::Edit { .. } => {
            crate::mount_diff::classify_mount_diffs(&editor.original.mounts, &editor.pending.mounts)
                .into_iter()
                .map(|diff| match diff {
                    crate::mount_diff::MountDiff::Added(mount) => WorkspaceMountDiff::Added(
                        workspace_mount_preview_row(mount, &editor.mount_info_cache),
                    ),
                    crate::mount_diff::MountDiff::Removed(mount) => WorkspaceMountDiff::Removed(
                        workspace_mount_preview_row(mount, &editor.mount_info_cache),
                    ),
                    crate::mount_diff::MountDiff::Modified { original, pending } => {
                        WorkspaceMountDiff::Modified {
                            original: workspace_mount_preview_row(
                                original,
                                &editor.mount_info_cache,
                            ),
                            pending: workspace_mount_preview_row(pending, &editor.mount_info_cache),
                        }
                    }
                    crate::mount_diff::MountDiff::Unchanged(_) => WorkspaceMountDiff::Unchanged,
                })
                .collect()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceMountDiff {
    Added(WorkspaceMountPreviewRow),
    Removed(WorkspaceMountPreviewRow),
    Modified {
        original: WorkspaceMountPreviewRow,
        pending: WorkspaceMountPreviewRow,
    },
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMountPreviewRow {
    pub src: String,
    pub dst: String,
    pub readonly: bool,
    pub isolation: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceAuthChange {
    pub label: String,
    pub original: String,
    pub pending: String,
}

#[must_use]
pub fn workspace_auth_change(
    label_prefix: &str,
    field: &str,
    original: &str,
    pending: &str,
) -> WorkspaceAuthChange {
    WorkspaceAuthChange {
        label: format!("{label_prefix} {field}"),
        original: original.to_owned(),
        pending: pending.to_owned(),
    }
}

#[must_use]
pub fn credential_presence(
    config: &jackin_config::AppConfig,
    workspace_name: &str,
    role: &str,
    kind: AuthKind,
    mode: AuthMode,
) -> bool {
    let Some(env_name) = kind.required_env_var(mode) else {
        return false;
    };
    panel_auth_source_value(config, workspace_name, role, env_name, kind).is_some()
}

#[must_use]
pub const fn credential_label(present: bool) -> &'static str {
    if present { "(set)" } else { "(unset)" }
}

#[must_use]
pub fn source_folder_text(display: &AuthSourceFolderDisplay) -> String {
    match display.kind {
        AuthSourceFolderKind::Default => format!("default: {}", display.path),
        AuthSourceFolderKind::Explicit => display.path.clone(),
        AuthSourceFolderKind::Inherited => format!("inherited: {}", display.path),
    }
}

#[must_use]
pub fn workspace_env_preview(workspace: &jackin_config::WorkspaceConfig) -> SettingsEnvPreview {
    SettingsEnvPreview {
        env: env_display_map_without_auth_credentials(&workspace.env),
        roles: workspace
            .roles
            .iter()
            .map(|(role, config)| {
                (
                    role.clone(),
                    env_display_map_without_auth_credentials(&config.env),
                )
            })
            .collect(),
    }
}

#[must_use]
pub fn role_auth_relevant(
    original: &jackin_config::WorkspaceConfig,
    pending: &jackin_config::WorkspaceConfig,
    role: &str,
    kind: AuthKind,
) -> bool {
    let original_role = original.roles.get(role);
    let pending_role = pending.roles.get(role);
    role_auth_mode_and_credential(original_role, kind)
        != role_auth_mode_and_credential(pending_role, kind)
        || role_sync_source_dir_text(original_role, kind)
            != role_sync_source_dir_text(pending_role, kind)
}

fn role_sync_source_dir_text(
    role: Option<&jackin_config::WorkspaceRoleOverride>,
    kind: AuthKind,
) -> Option<String> {
    let agent = auth_kind_agent(kind)?;
    role.and_then(|role| role.sync_source_dir_for(agent))
        .map(|path| path.display().to_string())
}

#[must_use]
pub fn workspace_auth_changes(
    config: &jackin_config::AppConfig,
    workspace_name: &str,
    original: &jackin_config::WorkspaceConfig,
    pending: &jackin_config::WorkspaceConfig,
) -> Vec<WorkspaceAuthChange> {
    let original_cfg = config_with_workspace(config, workspace_name, original.clone());
    let pending_cfg = config_with_workspace(config, workspace_name, pending.clone());
    let mut changes = Vec::new();

    for kind in AuthKind::WORKSPACE_PANEL_KINDS {
        push_auth_layer_changes(
            &mut changes,
            kind.label().to_owned(),
            &original_cfg,
            &pending_cfg,
            workspace_name,
            "",
            *kind,
        );
    }

    let role_names: BTreeSet<String> = original
        .roles
        .keys()
        .chain(pending.roles.keys())
        .cloned()
        .collect();
    for role in role_names {
        for kind in AuthKind::WORKSPACE_PANEL_KINDS {
            if !role_auth_relevant(original, pending, &role, *kind) {
                continue;
            }
            push_auth_layer_changes(
                &mut changes,
                format!("Role {role} / {}", kind.label()),
                &original_cfg,
                &pending_cfg,
                workspace_name,
                &role,
                *kind,
            );
        }
    }

    changes
}

fn config_with_workspace(
    config: &jackin_config::AppConfig,
    workspace_name: &str,
    workspace: jackin_config::WorkspaceConfig,
) -> jackin_config::AppConfig {
    let mut next = config.clone();
    next.workspaces.insert(workspace_name.to_owned(), workspace);
    next
}

fn push_auth_layer_changes(
    changes: &mut Vec<WorkspaceAuthChange>,
    label_prefix: String,
    original_cfg: &jackin_config::AppConfig,
    pending_cfg: &jackin_config::AppConfig,
    workspace_name: &str,
    role: &str,
    kind: AuthKind,
) {
    let original_mode = resolve_panel_mode(original_cfg, kind, workspace_name, role);
    let pending_mode = resolve_panel_mode(pending_cfg, kind, workspace_name, role);
    if original_mode != pending_mode {
        changes.push(workspace_auth_change(
            &label_prefix,
            "mode",
            original_mode.as_str(),
            pending_mode.as_str(),
        ));
    }

    let original_credential =
        credential_presence(original_cfg, workspace_name, role, kind, original_mode);
    let pending_credential =
        credential_presence(pending_cfg, workspace_name, role, kind, pending_mode);
    if original_credential != pending_credential {
        changes.push(workspace_auth_change(
            &label_prefix,
            "credential",
            credential_label(original_credential),
            credential_label(pending_credential),
        ));
    }

    if auth_kind_agent(kind).is_some()
        && (auth_mode_supports_source_folder(kind, original_mode)
            || auth_mode_supports_source_folder(kind, pending_mode))
    {
        let original_source = source_folder_text(&editor_source_folder_display(
            original_cfg,
            workspace_name,
            role,
            kind,
        ));
        let pending_source = source_folder_text(&editor_source_folder_display(
            pending_cfg,
            workspace_name,
            role,
            kind,
        ));
        if original_source != pending_source {
            changes.push(workspace_auth_change(
                &label_prefix,
                "source folder",
                &original_source,
                &pending_source,
            ));
        }
    }
}

impl WorkspaceMountPreviewRow {
    #[must_use]
    pub fn summary(&self) -> String {
        let mode = if self.readonly { "ro" } else { "rw" };
        let host = if self.src == self.dst {
            String::new()
        } else {
            format!("  host: {}", self.src)
        };
        format!(
            "{}{host}  ({mode}, {}, {})",
            self.dst, self.isolation, self.kind
        )
    }
}

#[must_use]
pub fn workspace_mount_preview_row(
    mount: &jackin_config::MountConfig,
    cache: &crate::mount_info_cache::MountInfoCache,
) -> WorkspaceMountPreviewRow {
    WorkspaceMountPreviewRow {
        src: jackin_tui::shorten_home(&mount.src),
        dst: jackin_tui::shorten_home(&mount.dst),
        readonly: mount.readonly,
        isolation: mount.isolation.as_str().to_owned(),
        kind: cache.label(&mount.src),
    }
}
