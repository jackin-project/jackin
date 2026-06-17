//! Save-confirm preview line builders.
//!
//! Input handlers decide when a save preview opens; this module owns the
//! Ratatui line composition for the preview dialogs.

use crate::config::AppConfig;
use crate::console::tui::state::{EditorMode, EditorState};
use jackin_console::tui::auth::{AuthKind, auth_mode_supports_source_folder};
use jackin_console::tui::auth_config::{
    auth_kind_agent, editor_source_folder_display, env_display_map, resolve_panel_mode,
};
use jackin_console::tui::components::save_preview::{
    credential_label, credential_presence, global_mount_preview_row, role_auth_relevant,
    settings_env_preview, source_folder_text, workspace_auth_change, workspace_env_preview,
    workspace_mount_preview_row,
};

#[cfg(test)]
mod tests;

pub(crate) fn build_confirm_save_lines(
    editor: &EditorState<'_>,
    config: &AppConfig,
    collapse_lines: &[ratatui::text::Line<'static>],
) -> Vec<ratatui::text::Line<'static>> {
    jackin_console::tui::components::save_preview::workspace_save_lines(&workspace_save_preview(
        editor,
        config,
        collapse_lines,
    ))
}

fn workspace_save_preview(
    editor: &EditorState<'_>,
    config: &AppConfig,
    collapse_lines: &[ratatui::text::Line<'static>],
) -> jackin_console::tui::components::save_preview::WorkspaceSavePreview {
    use jackin_console::tui::components::save_preview::{
        WorkspaceMountDiff, WorkspaceSaveMode, WorkspaceSavePreview,
    };

    let mode = match &editor.mode {
        EditorMode::Create => WorkspaceSaveMode::Create {
            name: jackin_console::tui::components::save_preview::workspace_create_display_name(
                editor.pending_name.as_deref(),
            ),
        },
        EditorMode::Edit { name } => WorkspaceSaveMode::Edit {
            original_name: name.clone(),
            display_name: editor.pending_name.clone().unwrap_or_else(|| name.clone()),
            pending_name: editor.pending_name.clone(),
        },
    };

    let mount_diffs = match editor.mode {
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
        EditorMode::Edit { .. } => crate::console::tui::state::classify_mount_diffs(
            &editor.original.mounts,
            &editor.pending.mounts,
        )
        .into_iter()
        .map(|diff| match diff {
            crate::console::tui::state::MountDiff::Added(mount) => WorkspaceMountDiff::Added(
                workspace_mount_preview_row(mount, &editor.mount_info_cache),
            ),
            crate::console::tui::state::MountDiff::Removed(mount) => WorkspaceMountDiff::Removed(
                workspace_mount_preview_row(mount, &editor.mount_info_cache),
            ),
            crate::console::tui::state::MountDiff::Modified { original, pending } => {
                WorkspaceMountDiff::Modified {
                    original: workspace_mount_preview_row(original, &editor.mount_info_cache),
                    pending: workspace_mount_preview_row(pending, &editor.mount_info_cache),
                }
            }
            crate::console::tui::state::MountDiff::Unchanged(_) => WorkspaceMountDiff::Unchanged,
        })
        .collect(),
    };

    WorkspaceSavePreview {
        mode,
        original_workdir: matches!(editor.mode, EditorMode::Edit { .. })
            .then(|| crate::tui::shorten_home(&editor.original.workdir)),
        pending_workdir: crate::tui::shorten_home(&editor.pending.workdir),
        mount_diffs,
        auth_changes: workspace_auth_changes(editor, config),
        original_allowed_roles: editor.original.allowed_roles.clone(),
        pending_allowed_roles: editor.pending.allowed_roles.clone(),
        role_count: config.roles.len(),
        original_default_role: editor.original.default_role.clone(),
        pending_default_role: editor.pending.default_role.clone(),
        original_keep_awake: editor.original.keep_awake.enabled,
        pending_keep_awake: editor.pending.keep_awake.enabled,
        original_git_pull: editor.original.git_pull_on_entry,
        pending_git_pull: editor.pending.git_pull_on_entry,
        env_original: workspace_env_preview(&editor.original),
        env_pending: workspace_env_preview(&editor.pending),
        collapse_lines: collapse_lines.to_vec(),
    }
}

fn workspace_auth_changes(
    editor: &EditorState<'_>,
    config: &AppConfig,
) -> Vec<jackin_console::tui::components::save_preview::WorkspaceAuthChange> {
    let workspace_name = match &editor.mode {
        EditorMode::Edit { name } => name.as_str(),
        EditorMode::Create => editor.pending_name.as_deref().unwrap_or("(new workspace)"),
    };
    let original_cfg = config_with_workspace(config, workspace_name, editor.original.clone());
    let pending_cfg = config_with_workspace(config, workspace_name, editor.pending.clone());
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

    let role_names: std::collections::BTreeSet<String> = editor
        .original
        .roles
        .keys()
        .chain(editor.pending.roles.keys())
        .cloned()
        .collect();
    for role in role_names {
        for kind in AuthKind::WORKSPACE_PANEL_KINDS {
            if !role_auth_relevant(&editor.original, &editor.pending, &role, *kind) {
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
    config: &AppConfig,
    workspace_name: &str,
    workspace: crate::workspace::WorkspaceConfig,
) -> AppConfig {
    let mut next = config.clone();
    next.workspaces.insert(workspace_name.to_owned(), workspace);
    next
}

fn push_auth_layer_changes(
    changes: &mut Vec<jackin_console::tui::components::save_preview::WorkspaceAuthChange>,
    label_prefix: String,
    original_cfg: &AppConfig,
    pending_cfg: &AppConfig,
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

/// Append `+ KEY = VALUE` / `- KEY` lines to `out` for the diff between
/// two env maps. `indent` (`None` or `Some("  ")`) controls per-role
/// sub-indent — workspace-level lines use two spaces to match existing
/// diff styling; per-role lines nest one extra level.
#[cfg(test)]
pub(crate) fn append_env_map_diff_lines(
    out: &mut Vec<ratatui::text::Line<'static>>,
    indent: Option<&str>,
    original: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    pending: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    value: ratatui::style::Style,
    dim: ratatui::style::Style,
) {
    let original = env_display_map(original);
    let pending = env_display_map(pending);
    jackin_console::tui::components::save_preview::append_env_map_diff_lines(
        out, indent, &original, &pending, value, dim,
    );
}

pub(crate) fn collapse_section_lines(
    collapses: &[crate::workspace::Removal],
) -> Vec<ratatui::text::Line<'static>> {
    let display_pairs: Vec<_> = collapses
        .iter()
        .map(|r| {
            (
                crate::tui::shorten_home(&r.child.src),
                crate::tui::shorten_home(&r.covered_by.src),
            )
        })
        .collect();
    jackin_console::tui::components::save_preview::collapse_section_lines(&display_pairs)
}

// ── Settings save preview ─────────────────────────────────────────────────────

/// Build the diff preview lines for the settings save confirmation dialog.
/// Mirrors the format of `build_confirm_save_lines` for the workspace editor.
/// Shows a summary section (counts per category) followed by per-category diffs.
#[must_use]
pub(crate) fn build_settings_save_lines(
    settings: &crate::console::tui::state::SettingsState<'_>,
) -> Vec<ratatui::text::Line<'static>> {
    jackin_console::tui::components::save_preview::settings_save_lines(&settings_save_preview(
        settings,
    ))
}

fn settings_save_preview(
    settings: &crate::console::tui::state::SettingsState<'_>,
) -> jackin_console::tui::components::save_preview::SettingsSavePreview {
    use jackin_console::tui::components::save_preview::{
        AuthPreviewRow, SettingsGeneralPreview, SettingsSavePreview, TrustPreviewRow,
    };

    SettingsSavePreview {
        general: SettingsGeneralPreview {
            original_coauthor_trailer: settings.general.original_coauthor_trailer,
            pending_coauthor_trailer: settings.general.pending_coauthor_trailer,
            original_dco: settings.general.original_dco,
            pending_dco: settings.general.pending_dco,
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
