//! Non-TUI config persistence services.

use std::collections::{BTreeMap, BTreeSet};

use crate::config::{AppConfig, EnvScope, GlobalMountRow, RoleSource};
use crate::console::tui::state::{SettingsAuthRow, SettingsEnvConfig, SettingsTrustRow};
use crate::operator_env::EnvValue;
use crate::paths::JackinPaths;
use crate::workspace::WorkspaceConfig;
use jackin_console::services::config_save::{WorkspaceSaveDiffOp, workspace_save_diff_plan};
use jackin_console::tui::auth::AuthKind;
use jackin_console::tui::auth_config::{
    auth_kind_agent, auth_mode_to_auth_forward, auth_mode_to_github,
};

#[cfg(test)]
mod tests;

/// Upsert one role source into the operator config and reload the saved model.
pub(crate) fn upsert_role_source(
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: &str,
    source: &RoleSource,
) -> anyhow::Result<()> {
    let mut editor_doc = crate::config::ConfigEditor::open(paths)?;
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
    let mut editor_doc = crate::config::ConfigEditor::open(paths)?;
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
    let mut editor_doc = crate::config::ConfigEditor::open(paths)?;
    for row in original {
        editor_doc.remove_mount(&row.name, row.scope.as_deref());
    }
    for row in pending {
        editor_doc.add_mount(&row.name, row.mount.clone(), row.scope.as_deref());
    }
    editor_doc.save()
}

pub(crate) struct SettingsSaveInput<'a> {
    pub mounts_original: &'a [GlobalMountRow],
    pub mounts_pending: &'a [GlobalMountRow],
    pub env_original: &'a SettingsEnvConfig,
    pub env_pending: &'a SettingsEnvConfig,
    pub auth_pending: &'a [SettingsAuthRow],
    pub original_github_env: &'a BTreeMap<String, EnvValue>,
    pub github_env: &'a BTreeMap<String, EnvValue>,
    pub trust_pending: &'a [SettingsTrustRow],
    pub git_coauthor_trailer: bool,
    pub git_dco: bool,
}

/// Save all settings tabs and return the reloaded config model.
#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
pub(crate) fn save_settings(
    paths: &JackinPaths,
    input: SettingsSaveInput<'_>,
) -> anyhow::Result<AppConfig> {
    AppConfig::validate_global_mount_rows(input.mounts_pending)?;
    validate_settings_env(input.env_pending, input.trust_pending)?;
    let mut editor_doc = crate::config::ConfigEditor::open(paths)?;

    for row in input.mounts_original {
        editor_doc.remove_mount(&row.name, row.scope.as_deref());
    }
    for row in input.mounts_pending {
        editor_doc.add_mount(&row.name, row.mount.clone(), row.scope.as_deref());
    }

    for key in input.env_original.env.keys() {
        editor_doc.remove_env_var(&EnvScope::Global, key);
    }
    for (role, env) in &input.env_original.roles {
        for key in env.keys() {
            editor_doc.remove_env_var(&EnvScope::Role(role.clone()), key);
        }
    }
    for (key, value) in &input.env_pending.env {
        editor_doc.set_env_var(&EnvScope::Global, key, value.clone())?;
    }
    for (role, env) in &input.env_pending.roles {
        for (key, value) in env {
            editor_doc.set_env_var(&EnvScope::Role(role.clone()), key, value.clone())?;
        }
    }

    for row in input.auth_pending {
        match row.kind {
            AuthKind::Claude
            | AuthKind::Codex
            | AuthKind::Amp
            | AuthKind::Kimi
            | AuthKind::Opencode
            | AuthKind::Grok => {
                let Some(agent) = auth_kind_agent(row.kind) else {
                    continue;
                };
                if !row.kind.supported_modes().contains(&row.mode) {
                    anyhow::bail!(
                        "auth mode {} is not supported for {}",
                        row.mode.as_str(),
                        row.kind.label()
                    );
                }
                let Some(mode) = auth_mode_to_auth_forward(row.mode) else {
                    anyhow::bail!(
                        "auth mode {} is not supported for {}",
                        row.mode.as_str(),
                        row.kind.label()
                    );
                };
                editor_doc.set_global_auth_forward(agent, mode);
                editor_doc.set_global_sync_source_dir(agent, row.sync_source_dir.as_deref());
            }
            AuthKind::Github => {
                let Some(mode) = auth_mode_to_github(row.mode) else {
                    anyhow::bail!(
                        "auth mode {} is not supported for {}",
                        row.mode.as_str(),
                        row.kind.label()
                    );
                };
                editor_doc.set_global_github_auth_forward(mode);
            }
            // Provider-credential kinds are env-only; the credential lives in the
            // env_vars block and is written via the env commit path above.
            AuthKind::Zai | AuthKind::Minimax => {}
        }
    }
    for key in input.original_github_env.keys() {
        editor_doc.remove_global_github_env_var(key);
    }
    for (key, value) in input.github_env {
        editor_doc.set_global_github_env_var(key, value.clone())?;
    }

    for row in input.trust_pending {
        editor_doc.set_agent_trust(&row.role, row.trusted);
    }

    editor_doc.set_git_coauthor_trailer(input.git_coauthor_trailer);
    editor_doc.set_git_dco(input.git_dco);

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
    let mut editor_doc = crate::config::ConfigEditor::open(paths)?;
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

            let mut edit =
                crate::console::domain::build_workspace_edit(input.original, input.pending);
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
    editor_doc: &mut crate::config::ConfigEditor,
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

fn validate_settings_env(
    env: &SettingsEnvConfig,
    roles: &[SettingsTrustRow],
) -> anyhow::Result<()> {
    let registered: BTreeSet<&str> = roles.iter().map(|r| r.role.as_str()).collect();
    validate_settings_env_keys("global", env.env.keys())?;
    for (role, role_env) in &env.roles {
        if !registered.contains(role.as_str()) {
            anyhow::bail!("role {role:?} is not registered");
        }
        validate_settings_env_keys(role, role_env.keys())?;
    }
    Ok(())
}

#[allow(unfulfilled_lint_expectations)]
#[expect(
    single_use_lifetimes,
    reason = "impl Iterator over borrowed String keys cannot use anonymous lifetimes on stable Rust"
)]
fn validate_settings_env_keys<'a>(
    scope: &str,
    keys: impl Iterator<Item = &'a String>,
) -> anyhow::Result<()> {
    for key in keys {
        if key.trim().is_empty() {
            anyhow::bail!("env var key cannot be empty");
        }
        if crate::env_model::is_reserved(key) {
            anyhow::bail!(
                "env name {key:?} in {scope} is reserved by the jackin runtime and cannot be set"
            );
        }
    }
    Ok(())
}
