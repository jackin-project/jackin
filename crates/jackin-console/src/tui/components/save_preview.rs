// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Save-confirm preview line builders for console-local dialogs.

use std::collections::{BTreeMap, BTreeSet};

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::components::editor_rows::{AuthSourceFolderDisplay, AuthSourceFolderKind};
use crate::tui::screens::editor::model::{EditorMode, EditorState};
use crate::tui::screens::settings::model::{
    GlobalMountsState, SettingsAuthState, SettingsEnvState, SettingsState, SettingsTrustState,
};
use crate::tui::{
    auth::{AuthKind, AuthMode, auth_mode_supports_source_folder},
    auth_config::{
        auth_kind_agent, editor_source_folder_display, env_display_map,
        env_display_map_without_auth_credentials, panel_auth_source_value, resolve_panel_mode,
        role_auth_mode_and_credential,
    },
};

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
    AuthFormTarget,
    PendingTokenGenerate,
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
        src: jackin_core::shorten_home(&mount.src),
        dst: jackin_core::shorten_home(&mount.dst),
        readonly: mount.readonly,
        isolation: mount.isolation.as_str().to_owned(),
        kind: cache.label(&mount.src),
    }
}

#[must_use]
pub fn collapse_section_lines(collapses: &[(String, String)]) -> Vec<Line<'static>> {
    let style = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    collapses
        .iter()
        .map(|(child, parent)| {
            Line::from(Span::styled(
                format!("  {child} will be subsumed under {parent}"),
                style,
            ))
        })
        .collect()
}

#[must_use]
pub fn collapse_removal_lines(collapses: &[jackin_config::Removal]) -> Vec<Line<'static>> {
    let display_pairs: Vec<_> = collapses
        .iter()
        .map(|removal| {
            (
                jackin_core::shorten_home(&removal.child.src),
                jackin_core::shorten_home(&removal.covered_by.src),
            )
        })
        .collect();
    collapse_section_lines(&display_pairs)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsSavePreview {
    pub general: SettingsGeneralPreview,
    pub mounts_original: Vec<MountPreviewRow>,
    pub mounts_pending: Vec<MountPreviewRow>,
    pub env_original: SettingsEnvPreview,
    pub env_pending: SettingsEnvPreview,
    pub auth_original: Vec<AuthPreviewRow>,
    pub auth_pending: Vec<AuthPreviewRow>,
    pub auth_github_env_original: BTreeMap<String, String>,
    pub auth_github_env_pending: BTreeMap<String, String>,
    pub trust_original: Vec<TrustPreviewRow>,
    pub trust_pending: Vec<TrustPreviewRow>,
}

pub type ConsoleSettingsState<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
> = SettingsState<
    GlobalMountsState<jackin_config::GlobalMountRow, MountModal>,
    SettingsEnvState<jackin_config::EnvValue, EnvModal>,
    SettingsAuthState<jackin_config::EnvValue, AuthModal, PendingOpCommit>,
    SettingsTrustState,
    ErrorPopup,
    PendingToken,
>;

#[must_use]
pub fn settings_save_preview<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    settings: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
) -> SettingsSavePreview {
    SettingsSavePreview {
        general: SettingsGeneralPreview {
            original_toggles: SettingsGeneralToggles {
                coauthor_trailer: settings.general.original_coauthor_trailer,
                dco: settings.general.original_dco,
            },
            pending_toggles: SettingsGeneralToggles {
                coauthor_trailer: settings.general.pending_coauthor_trailer,
                dco: settings.general.pending_dco,
            },
        },
        mounts_original: settings
            .mounts
            .original
            .iter()
            .map(global_mount_preview_row)
            .collect(),
        mounts_pending: settings
            .mounts
            .pending
            .iter()
            .map(global_mount_preview_row)
            .collect(),
        env_original: settings_env_preview(&settings.env.original),
        env_pending: settings_env_preview(&settings.env.pending),
        auth_original: settings
            .auth
            .original
            .iter()
            .map(|row| AuthPreviewRow {
                label: row.kind.label().to_owned(),
                mode: row.mode.as_str().to_owned(),
            })
            .collect(),
        auth_pending: settings
            .auth
            .pending
            .iter()
            .map(|row| AuthPreviewRow {
                label: row.kind.label().to_owned(),
                mode: row.mode.as_str().to_owned(),
            })
            .collect(),
        auth_github_env_original: env_display_map(&settings.auth.original_github_env),
        auth_github_env_pending: env_display_map(&settings.auth.github_env),
        trust_original: settings
            .trust
            .original
            .iter()
            .map(|row| TrustPreviewRow {
                role: row.role.clone(),
                trusted: row.trusted,
            })
            .collect(),
        trust_pending: settings
            .trust
            .pending
            .iter()
            .map(|row| TrustPreviewRow {
                role: row.role.clone(),
                trusted: row.trusted,
            })
            .collect(),
    }
}

#[must_use]
pub fn build_settings_save_lines<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingToken,
    PendingOpCommit,
>(
    settings: &ConsoleSettingsState<
        MountModal,
        EnvModal,
        AuthModal,
        ErrorPopup,
        PendingToken,
        PendingOpCommit,
    >,
) -> Vec<Line<'static>> {
    settings_save_lines(&settings_save_preview(settings))
}

/// Toggle pair (git coauthor trailer + DCO enforcement) that the settings
/// dialog captures at edit time. Bundled so the parent `SettingsGeneralPreview`
/// keeps the `struct_excessive_bools` clippy gate quiet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SettingsGeneralToggles {
    pub coauthor_trailer: bool,
    pub dco: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsGeneralPreview {
    pub original_toggles: SettingsGeneralToggles,
    pub pending_toggles: SettingsGeneralToggles,
}

impl SettingsGeneralPreview {
    fn change_count(self) -> usize {
        usize::from(self.original_toggles.coauthor_trailer != self.pending_toggles.coauthor_trailer)
            + usize::from(self.original_toggles.dco != self.pending_toggles.dco)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountPreviewRow {
    pub scope: Option<String>,
    pub name: String,
    pub src: String,
    pub dst: String,
    pub readonly: bool,
}

#[must_use]
pub fn global_mount_preview_row(row: &jackin_config::GlobalMountRow) -> MountPreviewRow {
    MountPreviewRow {
        scope: row.scope.clone(),
        name: row.name.clone(),
        src: jackin_core::shorten_home(&row.mount.src),
        dst: jackin_core::shorten_home(&row.mount.dst),
        readonly: row.mount.readonly,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsEnvPreview {
    pub env: BTreeMap<String, String>,
    pub roles: BTreeMap<String, BTreeMap<String, String>>,
}

#[must_use]
pub fn settings_env_preview(
    config: &crate::tui::screens::settings::model::SettingsEnvConfig<jackin_config::EnvValue>,
) -> SettingsEnvPreview {
    SettingsEnvPreview {
        env: env_display_map(&config.env),
        roles: config
            .roles
            .iter()
            .map(|(role, env)| (role.clone(), env_display_map(env)))
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPreviewRow {
    pub label: String,
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustPreviewRow {
    pub role: String,
    pub trusted: bool,
}

#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "Workspace-save preview renderer: per-section (header / mount / auth / \
              env / role / status) line builder. Inline shape preserves the \
              per-section readability."
)]
pub fn workspace_save_lines(preview: &WorkspaceSavePreview) -> Vec<Line<'static>> {
    let heading = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let value = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);
    let dim = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);

    let mut out: Vec<Line<'static>> = Vec::new();

    match &preview.mode {
        WorkspaceSaveMode::Create { name } => {
            out.push(Line::from(vec![
                Span::styled("Create workspace: ", heading),
                Span::styled(name.clone(), value),
            ]));
            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled("Working directory: ", heading),
                Span::styled(preview.pending_workdir.clone(), value),
            ]));

            let mounts: Vec<_> = preview
                .mount_diffs
                .iter()
                .filter_map(|diff| match diff {
                    WorkspaceMountDiff::Added(row) => Some(row.summary()),
                    WorkspaceMountDiff::Removed(_)
                    | WorkspaceMountDiff::Modified { .. }
                    | WorkspaceMountDiff::Unchanged => None,
                })
                .collect();
            if !mounts.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled(
                    format!("Mounts ({}):", mounts.len()),
                    heading,
                )));
                for mount in mounts {
                    out.push(Line::from(Span::styled(
                        format!("  \u{2022} {mount}"),
                        value,
                    )));
                }
            }

            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled("Allowed roles: ", heading),
                Span::styled(allowed_roles_summary(preview), value),
            ]));
            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled("Default role: ", heading),
                Span::styled(
                    preview
                        .pending_default_role
                        .clone()
                        .unwrap_or_else(|| "(none)".into()),
                    value,
                ),
            ]));
            if preview.pending_toggles.keep_awake {
                out.push(Line::raw(""));
                out.push(Line::from(vec![
                    Span::styled("Keep awake: ", heading),
                    Span::styled("enabled", value),
                ]));
            }
            if preview.pending_toggles.git_pull {
                out.push(Line::raw(""));
                out.push(Line::from(vec![
                    Span::styled("Git pull: ", heading),
                    Span::styled("enabled", value),
                ]));
            }
            let env_lines =
                settings_env_diff_lines(&preview.env_original, &preview.env_pending, value, dim);
            if !env_lines.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Env vars:", heading)));
                out.extend(env_lines);
            }
            append_workspace_auth_lines(&mut out, &preview.auth_changes, heading, value, dim);
        }
        WorkspaceSaveMode::Edit {
            original_name,
            display_name,
            pending_name,
        } => {
            out.push(Line::from(vec![
                Span::styled("Edit workspace: ", heading),
                Span::styled(display_name.clone(), value),
            ]));

            if let Some(new_name) = pending_name
                && new_name != original_name
            {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Rename:", heading)));
                out.push(Line::from(Span::styled(
                    format!("  - {original_name}"),
                    dim,
                )));
                out.push(Line::from(Span::styled(format!("  + {new_name}"), value)));
            }

            if let Some(original_workdir) = &preview.original_workdir
                && original_workdir != &preview.pending_workdir
            {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Working directory:", heading)));
                out.push(Line::from(Span::styled(
                    format!("  - {original_workdir}"),
                    dim,
                )));
                out.push(Line::from(Span::styled(
                    format!("  + {}", preview.pending_workdir),
                    value,
                )));
            }

            if preview
                .mount_diffs
                .iter()
                .any(|diff| !matches!(diff, WorkspaceMountDiff::Unchanged))
            {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Mounts:", heading)));
                for diff in &preview.mount_diffs {
                    match diff {
                        WorkspaceMountDiff::Added(row) => {
                            let summary = row.summary();
                            out.push(Line::from(Span::styled(format!("  + {summary}"), value)));
                        }
                        WorkspaceMountDiff::Removed(row) => {
                            let summary = row.summary();
                            out.push(Line::from(Span::styled(format!("  - {summary}"), dim)));
                        }
                        WorkspaceMountDiff::Modified { original, pending } => {
                            let original = original.summary();
                            let pending = pending.summary();
                            out.push(Line::from(Span::styled(format!("  ~ {pending}"), value)));
                            out.push(Line::from(Span::styled(
                                format!("      was: {original}"),
                                dim,
                            )));
                        }
                        WorkspaceMountDiff::Unchanged => {}
                    }
                }
            }

            let added_roles: Vec<_> = preview
                .pending_allowed_roles
                .iter()
                .filter(|role| !preview.original_allowed_roles.contains(*role))
                .collect();
            let removed_roles: Vec<_> = preview
                .original_allowed_roles
                .iter()
                .filter(|role| !preview.pending_allowed_roles.contains(*role))
                .collect();
            if !added_roles.is_empty() || !removed_roles.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Allowed roles:", heading)));
                for role in added_roles {
                    out.push(Line::from(Span::styled(format!("  + {role}"), value)));
                }
                for role in removed_roles {
                    out.push(Line::from(Span::styled(format!("  - {role}"), dim)));
                }
            }

            if preview.pending_default_role != preview.original_default_role {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Default role:", heading)));
                if let Some(old) = &preview.original_default_role {
                    out.push(Line::from(Span::styled(format!("  - {old}"), dim)));
                }
                if let Some(new) = &preview.pending_default_role {
                    out.push(Line::from(Span::styled(format!("  + {new}"), value)));
                } else {
                    out.push(Line::from(Span::styled("  + (none)", value)));
                }
            }

            if preview.pending_toggles.keep_awake != preview.original_toggles.keep_awake {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Keep awake:", heading)));
                out.push(Line::from(Span::styled(
                    format!("  - {}", enabled_label(preview.original_toggles.keep_awake)),
                    dim,
                )));
                out.push(Line::from(Span::styled(
                    format!("  + {}", enabled_label(preview.pending_toggles.keep_awake)),
                    value,
                )));
            }

            if preview.pending_toggles.git_pull != preview.original_toggles.git_pull {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Git pull:", heading)));
                out.push(Line::from(Span::styled(
                    format!("  - {}", enabled_label(preview.original_toggles.git_pull)),
                    dim,
                )));
                out.push(Line::from(Span::styled(
                    format!("  + {}", enabled_label(preview.pending_toggles.git_pull)),
                    value,
                )));
            }

            let env_lines =
                settings_env_diff_lines(&preview.env_original, &preview.env_pending, value, dim);
            if !env_lines.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Env vars:", heading)));
                out.extend(env_lines);
            }
            append_workspace_auth_lines(&mut out, &preview.auth_changes, heading, value, dim);
        }
    }

    if !preview.collapse_lines.is_empty() {
        out.push(Line::raw(""));
        out.push(Line::from(Span::styled(
            "Mount collapse required:",
            heading,
        )));
        out.extend(preview.collapse_lines.iter().cloned());
    }

    out
}

fn append_workspace_auth_lines(
    out: &mut Vec<Line<'static>>,
    changes: &[WorkspaceAuthChange],
    heading: Style,
    value: Style,
    dim: Style,
) {
    if changes.is_empty() {
        return;
    }
    out.push(Line::raw(""));
    out.push(Line::from(Span::styled("Auth:", heading)));
    for change in changes {
        out.push(Line::from(Span::styled(
            format!("  {}", change.label),
            heading,
        )));
        out.push(Line::from(Span::styled(
            format!("    - {}", change.original),
            dim,
        )));
        out.push(Line::from(Span::styled(
            format!("    + {}", change.pending),
            value,
        )));
    }
}

fn allowed_roles_summary(preview: &WorkspaceSavePreview) -> String {
    if preview.pending_allowed_roles.is_empty() {
        return format!("any ({} roles)", preview.role_count);
    }
    preview.pending_allowed_roles.join(", ")
}

#[must_use]
pub fn settings_save_lines(preview: &SettingsSavePreview) -> Vec<Line<'static>> {
    let heading = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let add_style = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);
    let remove_style = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    let sep_style = Style::default().fg(jackin_tui::theme::PHOSPHOR_DARK);

    let mut out: Vec<Line<'static>> = Vec::new();

    out.push(Line::from(Span::styled("Save settings", heading)));
    out.push(Line::raw(""));

    let general_stats = settings_general_stats(preview.general);
    let mount_stats = settings_mount_stats(&preview.mounts_original, &preview.mounts_pending);
    let env_stats = settings_env_stats(&preview.env_original, &preview.env_pending);
    let auth_stats = settings_auth_stats(
        &preview.auth_original,
        &preview.auth_pending,
        &preview.auth_github_env_original,
        &preview.auth_github_env_pending,
    );
    let trust_stats = settings_trust_stats(&preview.trust_original, &preview.trust_pending);

    if let Some(s) = general_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  General:      ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }
    if let Some(s) = mount_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  Mounts:       ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }
    if let Some(s) = env_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  Environments: ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }
    if let Some(s) = auth_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  Auth:         ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }
    if let Some(s) = trust_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  Trust:        ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }

    out.push(Line::raw(""));
    out.push(Line::from(Span::styled("  \u{2500}".repeat(30), sep_style)));
    out.push(Line::raw(""));

    if general_stats.is_some() {
        out.push(Line::from(Span::styled("General:", heading)));
        let arrow = "\u{2192}";

        if preview.general.pending_toggles.coauthor_trailer
            != preview.general.original_toggles.coauthor_trailer
        {
            let from = enabled_label(preview.general.original_toggles.coauthor_trailer);
            let to = enabled_label(preview.general.pending_toggles.coauthor_trailer);
            out.push(Line::from(vec![
                Span::styled("  co-author trailer: ", heading),
                Span::styled(from, remove_style),
                Span::styled(format!(" {arrow} "), Style::default()),
                Span::styled(to, add_style),
            ]));
        }

        if preview.general.pending_toggles.dco != preview.general.original_toggles.dco {
            let from = enabled_label(preview.general.original_toggles.dco);
            let to = enabled_label(preview.general.pending_toggles.dco);
            out.push(Line::from(vec![
                Span::styled("  dco: ", heading),
                Span::styled(from, remove_style),
                Span::styled(format!(" {arrow} "), Style::default()),
                Span::styled(to, add_style),
            ]));
        }

        out.push(Line::raw(""));
    }

    let mount_lines = settings_mount_diff_lines(
        &preview.mounts_original,
        &preview.mounts_pending,
        add_style,
        remove_style,
    );
    if !mount_lines.is_empty() {
        out.push(Line::from(Span::styled("Mounts:", heading)));
        out.extend(mount_lines);
        out.push(Line::raw(""));
    }

    let env_lines = settings_env_diff_lines(
        &preview.env_original,
        &preview.env_pending,
        add_style,
        remove_style,
    );
    if !env_lines.is_empty() {
        out.push(Line::from(Span::styled("Environments:", heading)));
        out.extend(env_lines);
        out.push(Line::raw(""));
    }

    let auth_lines = settings_auth_diff_lines(
        &preview.auth_original,
        &preview.auth_pending,
        &preview.auth_github_env_original,
        &preview.auth_github_env_pending,
        add_style,
        remove_style,
    );
    if !auth_lines.is_empty() {
        out.push(Line::from(Span::styled("Auth:", heading)));
        out.extend(auth_lines);
        out.push(Line::raw(""));
    }

    let trust_lines = settings_trust_diff_lines(
        &preview.trust_original,
        &preview.trust_pending,
        add_style,
        remove_style,
    );
    if !trust_lines.is_empty() {
        out.push(Line::from(Span::styled("Trust:", heading)));
        out.extend(trust_lines);
        out.push(Line::raw(""));
    }

    while out
        .last()
        .is_some_and(|l| l.spans.is_empty() || l.spans.iter().all(|s| s.content.trim().is_empty()))
    {
        out.pop();
    }

    out
}

fn enabled_label(enabled: bool) -> &'static str {
    if enabled { "enabled" } else { "disabled" }
}

fn settings_general_stats(state: SettingsGeneralPreview) -> Option<String> {
    let count = state.change_count();
    if count == 0 {
        return None;
    }
    Some(if count == 1 {
        "1 change".to_owned()
    } else {
        format!("{count} changes")
    })
}

fn settings_mount_stats(
    original: &[MountPreviewRow],
    pending: &[MountPreviewRow],
) -> Option<String> {
    let (added, removed, modified) = mount_diff_counts(original, pending);
    summarize_diff_counts(added, removed, modified)
}

fn settings_env_stats(
    original: &SettingsEnvPreview,
    pending: &SettingsEnvPreview,
) -> Option<String> {
    let (added, removed, modified) = env_config_diff_counts(original, pending);
    summarize_diff_counts(added, removed, modified)
}

fn summarize_diff_counts(added: usize, removed: usize, modified: usize) -> Option<String> {
    if added + removed + modified == 0 {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    if added > 0 {
        parts.push(format!("{added} added"));
    }
    if removed > 0 {
        parts.push(format!("{removed} removed"));
    }
    if modified > 0 {
        parts.push(format!("{modified} modified"));
    }
    Some(parts.join(", "))
}

fn settings_auth_stats(
    original: &[AuthPreviewRow],
    pending: &[AuthPreviewRow],
    orig_github_env: &BTreeMap<String, String>,
    pend_github_env: &BTreeMap<String, String>,
) -> Option<String> {
    let row_changes = original
        .iter()
        .zip(pending.iter())
        .filter(|(a, b)| a.mode != b.mode)
        .count();
    let (env_added, env_removed, env_modified) =
        env_map_diff_counts(orig_github_env, pend_github_env);
    let total = row_changes + env_added + env_removed + env_modified;
    if total == 0 {
        return None;
    }
    Some(format!("{total} changed"))
}

fn settings_trust_stats(
    original: &[TrustPreviewRow],
    pending: &[TrustPreviewRow],
) -> Option<String> {
    let changed = original
        .iter()
        .zip(pending.iter())
        .filter(|(a, b)| a.trusted != b.trusted)
        .count();
    if changed == 0 {
        return None;
    }
    Some(format!("{changed} changed"))
}

fn mount_diff_counts(
    original: &[MountPreviewRow],
    pending: &[MountPreviewRow],
) -> (usize, usize, usize) {
    let orig_map = mount_map(original);
    let pend_map = mount_map(pending);
    let added = pend_map
        .keys()
        .filter(|k| !orig_map.contains_key(*k))
        .count();
    let removed = orig_map
        .keys()
        .filter(|k| !pend_map.contains_key(*k))
        .count();
    let modified = pend_map
        .iter()
        .filter(|(k, prow)| orig_map.get(*k).is_some_and(|orow| orow != *prow))
        .count();
    (added, removed, modified)
}

fn env_config_diff_counts(
    original: &SettingsEnvPreview,
    pending: &SettingsEnvPreview,
) -> (usize, usize, usize) {
    let (ga, gr, gm) = env_map_diff_counts(&original.env, &pending.env);
    let all_roles: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    let empty = BTreeMap::default();
    let (ra, rr, rm) = all_roles.into_iter().fold((0, 0, 0), |(a, r, m), role| {
        let oe = original.roles.get(role).unwrap_or(&empty);
        let pe = pending.roles.get(role).unwrap_or(&empty);
        let (da, dr, dm) = env_map_diff_counts(oe, pe);
        (a + da, r + dr, m + dm)
    });
    (ga + ra, gr + rr, gm + rm)
}

fn env_map_diff_counts(
    original: &BTreeMap<String, String>,
    pending: &BTreeMap<String, String>,
) -> (usize, usize, usize) {
    let added = pending
        .keys()
        .filter(|k| !original.contains_key(*k))
        .count();
    let removed = original
        .keys()
        .filter(|k| !pending.contains_key(*k))
        .count();
    let modified = pending
        .iter()
        .filter(|(k, v)| original.get(*k).is_some_and(|ov| ov != *v))
        .count();
    (added, removed, modified)
}

fn settings_mount_diff_lines(
    original: &[MountPreviewRow],
    pending: &[MountPreviewRow],
    add_style: Style,
    remove_style: Style,
) -> Vec<Line<'static>> {
    let orig_map = mount_map(original);
    let pend_map = mount_map(pending);

    let mut out: Vec<Line<'static>> = Vec::new();
    for (key, row) in &pend_map {
        if !orig_map.contains_key(key) {
            out.push(Line::from(Span::styled(
                format!("  + {}", mount_row_summary(row)),
                add_style,
            )));
        }
    }
    for (key, row) in &orig_map {
        if !pend_map.contains_key(key) {
            out.push(Line::from(Span::styled(
                format!("  - {}", mount_row_summary(row)),
                remove_style,
            )));
        }
    }
    for (key, prow) in &pend_map {
        if let Some(orow) = orig_map.get(key)
            && orow != prow
        {
            out.push(Line::from(Span::styled(
                format!("  ~ {}", mount_row_summary(prow)),
                add_style,
            )));
            out.push(Line::from(Span::styled(
                format!("      was: {}", mount_row_summary(orow)),
                remove_style,
            )));
        }
    }
    out
}

fn mount_map(rows: &[MountPreviewRow]) -> BTreeMap<(Option<String>, String), &MountPreviewRow> {
    rows.iter()
        .map(|row| ((row.scope.clone(), row.name.clone()), row))
        .collect()
}

fn mount_row_summary(row: &MountPreviewRow) -> String {
    let scope = row
        .scope
        .as_deref()
        .map(|s| format!("[{s}] "))
        .unwrap_or_default();
    let ro = if row.readonly { " (ro)" } else { "" };
    format!("{scope}{} \u{2192} {}{ro}", row.src, row.dst)
}

fn settings_env_diff_lines(
    original: &SettingsEnvPreview,
    pending: &SettingsEnvPreview,
    add_style: Style,
    remove_style: Style,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    append_env_map_diff_lines(
        &mut out,
        None,
        &original.env,
        &pending.env,
        add_style,
        remove_style,
    );
    let all_roles: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    let empty = BTreeMap::default();
    for role in all_roles {
        let oe = original.roles.get(role).unwrap_or(&empty);
        let pe = pending.roles.get(role).unwrap_or(&empty);
        let mut probe: Vec<Line<'static>> = Vec::new();
        append_env_map_diff_lines(&mut probe, None, oe, pe, add_style, remove_style);
        if !probe.is_empty() {
            out.push(Line::from(Span::styled(
                format!("  role {role}:"),
                add_style,
            )));
            append_env_map_diff_lines(&mut out, Some("  "), oe, pe, add_style, remove_style);
        }
    }
    out
}

pub fn append_env_map_diff_lines(
    out: &mut Vec<Line<'static>>,
    indent: Option<&str>,
    original: &BTreeMap<String, String>,
    pending: &BTreeMap<String, String>,
    value: Style,
    dim: Style,
) {
    let prefix = indent.unwrap_or("");
    for (k, v) in pending {
        match original.get(k) {
            Some(ov) if ov == v => {}
            _ => out.push(Line::from(Span::styled(
                format!("{prefix}  + {k} = {v}"),
                value,
            ))),
        }
    }
    for k in original.keys() {
        if !pending.contains_key(k) {
            out.push(Line::from(Span::styled(format!("{prefix}  - {k}"), dim)));
        }
    }
}

fn settings_auth_diff_lines(
    original: &[AuthPreviewRow],
    pending: &[AuthPreviewRow],
    orig_github_env: &BTreeMap<String, String>,
    pend_github_env: &BTreeMap<String, String>,
    add_style: Style,
    remove_style: Style,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    for (orig_row, pend_row) in original.iter().zip(pending.iter()) {
        if orig_row.mode != pend_row.mode {
            out.push(Line::from(Span::styled(
                format!(
                    "  ~ {}  {} \u{2192} {}",
                    pend_row.label, orig_row.mode, pend_row.mode
                ),
                add_style,
            )));
        }
    }
    append_env_map_diff_lines(
        &mut out,
        None,
        orig_github_env,
        pend_github_env,
        add_style,
        remove_style,
    );
    out
}

fn settings_trust_diff_lines(
    original: &[TrustPreviewRow],
    pending: &[TrustPreviewRow],
    add_style: Style,
    remove_style: Style,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    for (orig_row, pend_row) in original.iter().zip(pending.iter()) {
        if orig_row.trusted != pend_row.trusted {
            let (label, style) = if pend_row.trusted {
                (format!("  + {}  trusted", pend_row.role), add_style)
            } else {
                (format!("  - {}  untrusted", pend_row.role), remove_style)
            };
            out.push(Line::from(Span::styled(label, style)));
        }
    }
    out
}

#[cfg(test)]
mod tests;
