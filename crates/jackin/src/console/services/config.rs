//! Non-TUI config persistence services.

use crate::paths::JackinPaths;
use jackin_config::WorkspaceConfig;
use jackin_config::{AppConfig, RoleSource};
#[cfg(test)]
use jackin_config::GlobalMountRow;
use jackin_console::services::config_save::{
    WorkspaceSaveDiffOp, build_workspace_edit, workspace_save_diff_plan,
};

// Delegate settings-save to jackin-console; root callers keep the same path.
pub(crate) use jackin_console::services::config_save::{SettingsSaveInput, save_settings};

#[cfg(test)]
mod tests;

/// Upsert one role source into the operator config and reload the saved model.
pub(crate) fn upsert_role_source(
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: &str,
    source: &RoleSource,
) -> anyhow::Result<()> {
    let mut editor_doc = jackin_config::ConfigEditor::open(paths)?;
    editor_doc.upsert_agent_source(key, source);
    *config = editor_doc.save()?;
    Ok(())
}

/// Remove one saved workspace from operator config and reload the saved model.
pub(crate) fn remove_workspace(
    config: &mut AppConfig,
    paths: &JackinPaths,
    name: &str,
) -> anyhow::Result<()> {
    let mut editor_doc = jackin_config::ConfigEditor::open(paths)?;
    editor_doc.remove_workspace(name)?;
    *config = editor_doc.save()?;
    Ok(())
}

/// Save the global mount table and return the reloaded config model.
#[cfg(test)]
pub(crate) fn save_global_mounts(
    paths: &JackinPaths,
    original: &[GlobalMountRow],
    pending: &[GlobalMountRow],
) -> anyhow::Result<AppConfig> {
    AppConfig::validate_global_mount_rows(pending)?;
    let mut editor_doc = jackin_config::ConfigEditor::open(paths)?;
    for row in original {
        editor_doc.remove_mount(&row.name, row.scope.as_deref());
    }
    for row in pending {
        editor_doc.add_mount(&row.name, row.mount.clone(), row.scope.as_deref());
    }
    editor_doc.save()
}

pub(crate) enum WorkspaceSaveMode {
    Edit {
        original_name: String,
        pending_name: Option<String>,
        effective_removals: Vec<String>,
    },
    Create {
        name: String,
    },
}

pub(crate) struct WorkspaceSaveInput<'a> {
    pub mode: WorkspaceSaveMode,
    pub original: &'a WorkspaceConfig,
    pub pending: &'a WorkspaceConfig,
}

pub(crate) struct WorkspaceSaveResult {
    pub config: AppConfig,
    pub current_name: String,
    pub pending_rename: Option<String>,
}

/// Persist a workspace create/edit and return the reloaded config model.
#[allow(clippy::useless_let_if_seq)]
pub(crate) fn save_workspace(
    paths: &JackinPaths,
    input: WorkspaceSaveInput<'_>,
) -> anyhow::Result<WorkspaceSaveResult> {
    let mut editor_doc = jackin_config::ConfigEditor::open(paths)?;
    let (pending_rename, current_name) = match input.mode {
        WorkspaceSaveMode::Edit {
            original_name,
            pending_name,
            effective_removals,
        } => {
            let mut current_name = original_name;
            let mut rename_to = None;
            if let Some(new_name) = pending_name
                && new_name != current_name
            {
                editor_doc.rename_workspace(&current_name, &new_name)?;
                current_name.clone_from(&new_name);
                rename_to = Some(new_name);
            }

            let mut edit = build_workspace_edit(input.original, input.pending);
            edit.remove_destinations = effective_removals;
            editor_doc.edit_workspace(&current_name, edit)?;
            (rename_to, current_name)
        }
        WorkspaceSaveMode::Create { name } => {
            editor_doc.create_workspace(&name, input.pending.clone())?;
            (None, name)
        }
    };

    apply_workspace_save_diff_plan(
        &mut editor_doc,
        &current_name,
        input.original,
        input.pending,
    )?;
    let config = editor_doc.save()?;
    Ok(WorkspaceSaveResult {
        config,
        current_name,
        pending_rename,
    })
}

fn apply_workspace_save_diff_plan(
    editor_doc: &mut jackin_config::ConfigEditor,
    workspace_name: &str,
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) -> anyhow::Result<()> {
    for op in workspace_save_diff_plan(workspace_name, original, pending) {
        match op {
            WorkspaceSaveDiffOp::WorkspaceAuthForward { agent, mode } => {
                editor_doc.set_workspace_auth_forward(workspace_name, agent, mode);
            }
            WorkspaceSaveDiffOp::WorkspaceGithubAuthForward { mode } => {
                editor_doc.set_workspace_github_auth_forward(workspace_name, mode);
            }
            WorkspaceSaveDiffOp::WorkspaceRoleAuthForward { role, agent, mode } => {
                editor_doc.set_workspace_role_auth_forward(workspace_name, &role, agent, mode);
            }
            WorkspaceSaveDiffOp::WorkspaceRoleGithubAuthForward { role, mode } => {
                editor_doc.set_workspace_role_github_auth_forward(workspace_name, &role, mode);
            }
            WorkspaceSaveDiffOp::WorkspaceSyncSourceDir { agent, source } => {
                editor_doc.set_workspace_sync_source_dir(workspace_name, agent, source.as_deref());
            }
            WorkspaceSaveDiffOp::WorkspaceRoleSyncSourceDir {
                role,
                agent,
                source,
            } => {
                editor_doc.set_workspace_role_sync_source_dir(
                    workspace_name,
                    &role,
                    agent,
                    source.as_deref(),
                );
            }
            WorkspaceSaveDiffOp::EnvSet { scope, key, value } => {
                editor_doc.set_env_var(&scope, &key, value)?;
            }
            WorkspaceSaveDiffOp::EnvRemove { scope, key } => {
                let _ = editor_doc.remove_env_var(&scope, &key);
            }
        }
    }
    Ok(())
}
