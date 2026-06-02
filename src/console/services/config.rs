//! Non-TUI config persistence services.

use std::collections::{BTreeMap, BTreeSet};

use crate::config::{AppConfig, EnvScope, GlobalMountRow, RoleSource};
use crate::console::domain::{auth_kind_agent, auth_mode_to_auth_forward, auth_mode_to_github};
use crate::console::tui::state::{SettingsAuthRow, SettingsEnvConfig, SettingsTrustRow};
use crate::operator_env::EnvValue;
use crate::paths::JackinPaths;
use crate::workspace::WorkspaceConfig;
use jackin_console::tui::auth::AuthKind;

/// Upsert one role source into the operator config and reload the saved model.
pub fn upsert_role_source(
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
pub fn remove_workspace(
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
pub fn save_global_mounts(
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

pub struct SettingsSaveInput<'a> {
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
pub fn save_settings(
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
            | AuthKind::Opencode => {
                let Some(agent) = auth_kind_agent(row.kind) else {
                    continue;
                };
                let Some(mode) = auth_mode_to_auth_forward(row.mode) else {
                    anyhow::bail!(
                        "auth mode {} is not supported for {}",
                        row.mode.as_str(),
                        row.kind.label()
                    );
                };
                editor_doc.set_global_auth_forward(agent, mode);
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
            AuthKind::Zai => {}
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

pub enum WorkspaceSaveMode {
    Edit {
        original_name: String,
        pending_name: Option<String>,
        effective_removals: Vec<String>,
    },
    Create {
        name: String,
    },
}

pub struct WorkspaceSaveInput<'a> {
    pub mode: WorkspaceSaveMode,
    pub original: &'a WorkspaceConfig,
    pub pending: &'a WorkspaceConfig,
}

pub struct WorkspaceSaveResult {
    pub config: AppConfig,
    pub current_name: String,
    pub pending_rename: Option<String>,
}

/// Persist a workspace create/edit and return the reloaded config model.
pub fn save_workspace(
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
            let mut current_name = original_name.clone();
            let mut rename_to = None;
            if let Some(new_name) = pending_name
                && new_name != original_name
            {
                editor_doc.rename_workspace(&original_name, &new_name)?;
                current_name.clone_from(&new_name);
                rename_to = Some(new_name);
            }

            let mut edit =
                crate::console::domain::build_workspace_edit(input.original, input.pending);
            edit.remove_destinations = effective_removals;
            editor_doc.edit_workspace(&current_name, edit)?;
            apply_auth_forward_diff(
                &mut editor_doc,
                &current_name,
                input.original,
                input.pending,
            );
            (rename_to, current_name)
        }
        WorkspaceSaveMode::Create { name } => {
            editor_doc.create_workspace(&name, input.pending.clone())?;
            (None, name)
        }
    };

    apply_env_diff(
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

/// Reapply auth-forward deltas after `edit_workspace` rewrites the workspace table.
pub(crate) fn apply_auth_forward_diff(
    editor_doc: &mut crate::config::ConfigEditor,
    workspace_name: &str,
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) {
    use crate::agent::Agent;
    let original_claude = original.claude.as_ref().map(|c| c.auth_forward);
    let pending_claude = pending.claude.as_ref().map(|c| c.auth_forward);
    if original_claude != pending_claude {
        editor_doc.set_workspace_auth_forward(workspace_name, Agent::Claude, pending_claude);
    }
    let original_codex = original.codex.as_ref().map(|c| c.0.auth_forward);
    let pending_codex = pending.codex.as_ref().map(|c| c.0.auth_forward);
    if original_codex != pending_codex {
        editor_doc.set_workspace_auth_forward(workspace_name, Agent::Codex, pending_codex);
    }
    let original_amp = original.amp.as_ref().map(|c| c.0.auth_forward);
    let pending_amp = pending.amp.as_ref().map(|c| c.0.auth_forward);
    if original_amp != pending_amp {
        editor_doc.set_workspace_auth_forward(workspace_name, Agent::Amp, pending_amp);
    }
    let original_opencode = original.opencode.as_ref().map(|c| c.0.auth_forward);
    let pending_opencode = pending.opencode.as_ref().map(|c| c.0.auth_forward);
    if original_opencode != pending_opencode {
        editor_doc.set_workspace_auth_forward(workspace_name, Agent::Opencode, pending_opencode);
    }
    let original_github = original.github.as_ref().map(|g| g.auth_forward);
    let pending_github = pending.github.as_ref().map(|g| g.auth_forward);
    if original_github != pending_github {
        editor_doc.set_workspace_github_auth_forward(workspace_name, pending_github);
    }

    let role_keys: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    for role in role_keys {
        let orig_override = original.roles.get(role);
        let pend_override = pending.roles.get(role);
        let orig_claude = orig_override
            .and_then(|o| o.claude.as_ref())
            .map(|c| c.auth_forward);
        let pend_claude = pend_override
            .and_then(|p| p.claude.as_ref())
            .map(|c| c.auth_forward);
        if orig_claude != pend_claude {
            editor_doc.set_workspace_role_auth_forward(
                workspace_name,
                role,
                Agent::Claude,
                pend_claude,
            );
        }
        let orig_codex = orig_override
            .and_then(|o| o.codex.as_ref())
            .map(|c| c.0.auth_forward);
        let pend_codex = pend_override
            .and_then(|p| p.codex.as_ref())
            .map(|c| c.0.auth_forward);
        if orig_codex != pend_codex {
            editor_doc.set_workspace_role_auth_forward(
                workspace_name,
                role,
                Agent::Codex,
                pend_codex,
            );
        }
        let orig_amp = orig_override
            .and_then(|o| o.amp.as_ref())
            .map(|c| c.0.auth_forward);
        let pend_amp = pend_override
            .and_then(|p| p.amp.as_ref())
            .map(|c| c.0.auth_forward);
        if orig_amp != pend_amp {
            editor_doc.set_workspace_role_auth_forward(workspace_name, role, Agent::Amp, pend_amp);
        }
        let orig_opencode = orig_override
            .and_then(|o| o.opencode.as_ref())
            .map(|c| c.0.auth_forward);
        let pend_opencode = pend_override
            .and_then(|p| p.opencode.as_ref())
            .map(|c| c.0.auth_forward);
        if orig_opencode != pend_opencode {
            editor_doc.set_workspace_role_auth_forward(
                workspace_name,
                role,
                Agent::Opencode,
                pend_opencode,
            );
        }
        let orig_github = orig_override
            .and_then(|o| o.github.as_ref())
            .map(|g| g.auth_forward);
        let pend_github = pend_override
            .and_then(|p| p.github.as_ref())
            .map(|g| g.auth_forward);
        if orig_github != pend_github {
            editor_doc.set_workspace_role_github_auth_forward(workspace_name, role, pend_github);
        }
    }
}

fn apply_env_diff(
    editor_doc: &mut crate::config::ConfigEditor,
    workspace_name: &str,
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) -> anyhow::Result<()> {
    let ws_scope = EnvScope::Workspace(workspace_name.to_string());
    apply_env_map_diff(editor_doc, &ws_scope, &original.env, &pending.env)?;

    let empty = BTreeMap::<String, EnvValue>::new();
    let orig_ws_github_env = original.github.as_ref().map_or(&empty, |g| &g.env);
    let pend_ws_github_env = pending.github.as_ref().map_or(&empty, |g| &g.env);
    let ws_github_scope = EnvScope::WorkspaceGithub(workspace_name.to_string());
    apply_env_map_diff(
        editor_doc,
        &ws_github_scope,
        orig_ws_github_env,
        pend_ws_github_env,
    )?;

    let role_keys: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    for role in role_keys {
        let orig_env = original.roles.get(role).map_or(&empty, |o| &o.env);
        let pend_env = pending.roles.get(role).map_or(&empty, |p| &p.env);
        let scope = EnvScope::WorkspaceRole {
            workspace: workspace_name.to_string(),
            role: role.clone(),
        };
        apply_env_map_diff(editor_doc, &scope, orig_env, pend_env)?;

        let orig_role_github_env = original
            .roles
            .get(role)
            .and_then(|o| o.github.as_ref())
            .map_or(&empty, |g| &g.env);
        let pend_role_github_env = pending
            .roles
            .get(role)
            .and_then(|p| p.github.as_ref())
            .map_or(&empty, |g| &g.env);
        let role_github_scope = EnvScope::WorkspaceRoleGithub {
            workspace: workspace_name.to_string(),
            role: role.clone(),
        };
        apply_env_map_diff(
            editor_doc,
            &role_github_scope,
            orig_role_github_env,
            pend_role_github_env,
        )?;
    }
    Ok(())
}

fn apply_env_map_diff(
    editor_doc: &mut crate::config::ConfigEditor,
    scope: &EnvScope,
    original: &BTreeMap<String, EnvValue>,
    pending: &BTreeMap<String, EnvValue>,
) -> anyhow::Result<()> {
    for (key, value) in pending {
        match original.get(key) {
            Some(original_value) if original_value == value => {}
            _ => {
                editor_doc.set_env_var(scope, key, value.clone())?;
            }
        }
    }
    for key in original.keys() {
        if !pending.contains_key(key) {
            let _ = editor_doc.remove_env_var(scope, key);
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
