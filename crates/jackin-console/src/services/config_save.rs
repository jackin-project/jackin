//! Console-owned config save diff planning.
//!
//! The root binary still applies these operations through `ConfigEditor`, but
//! the rules for what changed between the original and pending console models
//! live with the console crate.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use jackin_config::{
    AppConfig, AuthForwardMode, EnvScope, EnvValue, GithubAuthMode, MountConfig, Removal,
    WorkspaceConfig, WorkspaceEdit, WorkspaceRoleOverride, plan_create, plan_edit,
};
use jackin_core::{Agent, WorkspaceName, env_model};
use jackin_tui::shorten_home;

use crate::tui::screens::settings::model::{SettingsEnvConfig, SettingsTrustRow};

const WORKSPACE_AUTH_AGENTS: [Agent; 6] = [
    Agent::Claude,
    Agent::Codex,
    Agent::Amp,
    Agent::Kimi,
    Agent::Opencode,
    Agent::Grok,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSaveDiffOp {
    WorkspaceAuthForward {
        agent: Agent,
        mode: Option<AuthForwardMode>,
    },
    WorkspaceGithubAuthForward {
        mode: Option<GithubAuthMode>,
    },
    WorkspaceRoleAuthForward {
        role: String,
        agent: Agent,
        mode: Option<AuthForwardMode>,
    },
    WorkspaceRoleGithubAuthForward {
        role: String,
        mode: Option<GithubAuthMode>,
    },
    WorkspaceSyncSourceDir {
        agent: Agent,
        source: Option<PathBuf>,
    },
    WorkspaceRoleSyncSourceDir {
        role: String,
        agent: Agent,
        source: Option<PathBuf>,
    },
    EnvSet {
        scope: EnvScope,
        key: String,
        value: EnvValue,
    },
    EnvRemove {
        scope: EnvScope,
        key: String,
    },
}

#[must_use]
pub fn workspace_save_diff_plan(
    workspace_name: &WorkspaceName,
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) -> Vec<WorkspaceSaveDiffOp> {
    let mut ops = Vec::new();
    push_auth_forward_diff(&mut ops, original, pending);
    push_sync_source_dir_diff(&mut ops, original, pending);
    push_env_diff(&mut ops, workspace_name, original, pending);
    ops
}

pub fn validate_settings_env<V>(
    env: &SettingsEnvConfig<V>,
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

/// Build the config-editor patch for a workspace edit from original/pending UI state.
#[must_use]
pub fn build_workspace_edit(
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) -> WorkspaceEdit {
    let mut edit = WorkspaceEdit::default();
    if pending.workdir != original.workdir {
        edit.workdir = Some(pending.workdir.clone());
    }
    for m in &pending.mounts {
        if !original.mounts.iter().any(|o| o == m) {
            edit.upsert_mounts.push(m.clone());
        }
    }
    for o in &original.mounts {
        if !pending.mounts.iter().any(|p| p.dst == o.dst) {
            edit.remove_destinations.push(o.dst.clone());
        }
    }
    for a in &pending.allowed_roles {
        if !original.allowed_roles.contains(a) {
            edit.allowed_roles_to_add.push(a.clone());
        }
    }
    for a in &original.allowed_roles {
        if !pending.allowed_roles.contains(a) {
            edit.allowed_roles_to_remove.push(a.clone());
        }
    }
    if pending.default_role != original.default_role {
        edit.default_role = Some(pending.default_role.clone());
    }
    if pending.keep_awake.enabled != original.keep_awake.enabled {
        edit.keep_awake_enabled = Some(pending.keep_awake.enabled);
    }
    if pending.git_pull_on_entry != original.git_pull_on_entry {
        edit.git_pull_on_entry_enabled = Some(pending.git_pull_on_entry);
    }
    edit
}

#[derive(Debug)]
pub enum EditorSavePreviewInput<'a> {
    Edit {
        original_name: &'a str,
        original: &'a WorkspaceConfig,
        pending: &'a WorkspaceConfig,
    },
    Create {
        pending: &'a WorkspaceConfig,
        pending_name: Option<&'a str>,
    },
}

#[derive(Debug)]
pub enum EditorSavePreviewPlan {
    Edit {
        effective_removals: Vec<String>,
        edit_driven_collapses: Vec<Removal>,
    },
    Create {
        final_mounts: Vec<MountConfig>,
        collapsed: Vec<Removal>,
    },
}

#[derive(Debug)]
pub enum EditorSavePreviewError {
    Message(String),
    PreExistingRedundantMounts {
        original_name: String,
        collapses: Vec<Removal>,
    },
}

#[must_use]
pub fn pre_existing_redundant_mounts_message(original_name: &str, collapses: &[Removal]) -> String {
    let details: Vec<String> = collapses
        .iter()
        .map(|r| {
            format!(
                "{} covered by {}",
                shorten_home(&r.child.src),
                shorten_home(&r.covered_by.src),
            )
        })
        .collect();
    format!(
        "pre-existing redundant mount(s) in this workspace: {}; \
         run `jackin❯ workspace prune {original_name}` to clean up",
        details.join(", "),
    )
}

#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
pub fn plan_editor_save_preview(
    config: &AppConfig,
    input: EditorSavePreviewInput<'_>,
) -> Result<EditorSavePreviewPlan, EditorSavePreviewError> {
    match input {
        EditorSavePreviewInput::Edit {
            original_name,
            original,
            pending,
        } => {
            let current_ws = config
                .workspaces
                .get(original_name)
                .cloned()
                .ok_or_else(|| {
                    EditorSavePreviewError::Message(format!(
                        "workspace {original_name:?} no longer exists in config"
                    ))
                })?;
            let edit_delta = build_workspace_edit(original, pending);
            let plan = plan_edit(
                &current_ws,
                &edit_delta.upsert_mounts,
                &edit_delta.remove_destinations,
                false,
            )
            .map_err(|e| EditorSavePreviewError::Message(e.to_string()))?;
            if plan.edit_driven_collapses.is_empty() && !plan.pre_existing_collapses.is_empty() {
                return Err(EditorSavePreviewError::PreExistingRedundantMounts {
                    original_name: original_name.to_owned(),
                    collapses: plan.pre_existing_collapses,
                });
            }
            Ok(EditorSavePreviewPlan::Edit {
                effective_removals: plan.effective_removals,
                edit_driven_collapses: plan.edit_driven_collapses,
            })
        }
        EditorSavePreviewInput::Create {
            pending,
            pending_name,
        } => {
            if pending_name.is_none() {
                return Err(EditorSavePreviewError::Message(
                    "missing workspace name".to_owned(),
                ));
            }
            let plan = plan_create(&pending.mounts)
                .map_err(|e| EditorSavePreviewError::Message(e.to_string()))?;
            Ok(EditorSavePreviewPlan::Create {
                final_mounts: plan.final_mounts,
                collapsed: plan.collapsed,
            })
        }
    }
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
        if env_model::is_reserved(key) {
            anyhow::bail!(
                "env name {key:?} in {scope} is reserved by the jackin runtime and cannot be set"
            );
        }
    }
    Ok(())
}

fn push_auth_forward_diff(
    ops: &mut Vec<WorkspaceSaveDiffOp>,
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) {
    for agent in WORKSPACE_AUTH_AGENTS {
        let original_mode = original.auth_forward_for(agent);
        let pending_mode = pending.auth_forward_for(agent);
        if original_mode != pending_mode {
            ops.push(WorkspaceSaveDiffOp::WorkspaceAuthForward {
                agent,
                mode: pending_mode,
            });
        }
    }
    let original_github = original.github.as_ref().map(|g| g.auth_forward);
    let pending_github = pending.github.as_ref().map(|g| g.auth_forward);
    if original_github != pending_github {
        ops.push(WorkspaceSaveDiffOp::WorkspaceGithubAuthForward {
            mode: pending_github,
        });
    }

    let role_keys: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    for role in role_keys {
        let orig_override = original.roles.get(role);
        let pend_override = pending.roles.get(role);
        for agent in WORKSPACE_AUTH_AGENTS {
            let original_mode = role_auth_forward_for(orig_override, agent);
            let pending_mode = role_auth_forward_for(pend_override, agent);
            if original_mode != pending_mode {
                ops.push(WorkspaceSaveDiffOp::WorkspaceRoleAuthForward {
                    role: role.clone(),
                    agent,
                    mode: pending_mode,
                });
            }
        }
        let orig_github = orig_override
            .and_then(|o| o.github.as_ref())
            .map(|g| g.auth_forward);
        let pend_github = pend_override
            .and_then(|p| p.github.as_ref())
            .map(|g| g.auth_forward);
        if orig_github != pend_github {
            ops.push(WorkspaceSaveDiffOp::WorkspaceRoleGithubAuthForward {
                role: role.clone(),
                mode: pend_github,
            });
        }
    }
}

fn push_sync_source_dir_diff(
    ops: &mut Vec<WorkspaceSaveDiffOp>,
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) {
    for agent in WORKSPACE_AUTH_AGENTS {
        let original_source = original.sync_source_dir_for(agent);
        let pending_source = pending.sync_source_dir_for(agent);
        if original_source != pending_source {
            ops.push(WorkspaceSaveDiffOp::WorkspaceSyncSourceDir {
                agent,
                source: pending_source,
            });
        }
    }

    let role_keys: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    for role in role_keys {
        let orig_override = original.roles.get(role);
        let pend_override = pending.roles.get(role);
        for agent in WORKSPACE_AUTH_AGENTS {
            let original_source = role_sync_source_dir_for(orig_override, agent);
            let pending_source = role_sync_source_dir_for(pend_override, agent);
            if original_source != pending_source {
                ops.push(WorkspaceSaveDiffOp::WorkspaceRoleSyncSourceDir {
                    role: role.clone(),
                    agent,
                    source: pending_source,
                });
            }
        }
    }
}

fn push_env_diff(
    ops: &mut Vec<WorkspaceSaveDiffOp>,
    workspace_name: &WorkspaceName,
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) {
    let ws_key = workspace_name.as_str().to_owned();
    let ws_scope = EnvScope::Workspace(ws_key.clone());
    push_env_map_diff(ops, ws_scope, &original.env, &pending.env);

    let empty = BTreeMap::<String, EnvValue>::new();
    let orig_ws_github_env = original.github.as_ref().map_or(&empty, |g| &g.env);
    let pend_ws_github_env = pending.github.as_ref().map_or(&empty, |g| &g.env);
    let ws_github_scope = EnvScope::WorkspaceGithub(ws_key.clone());
    push_env_map_diff(ops, ws_github_scope, orig_ws_github_env, pend_ws_github_env);

    let role_keys: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    for role in role_keys {
        let orig_env = original.roles.get(role).map_or(&empty, |o| &o.env);
        let pend_env = pending.roles.get(role).map_or(&empty, |p| &p.env);
        let scope = EnvScope::WorkspaceRole {
            workspace: ws_key.clone(),
            role: role.clone(),
        };
        push_env_map_diff(ops, scope, orig_env, pend_env);

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
            workspace: ws_key.clone(),
            role: role.clone(),
        };
        push_env_map_diff(
            ops,
            role_github_scope,
            orig_role_github_env,
            pend_role_github_env,
        );
    }
}

fn push_env_map_diff(
    ops: &mut Vec<WorkspaceSaveDiffOp>,
    scope: EnvScope,
    original: &BTreeMap<String, EnvValue>,
    pending: &BTreeMap<String, EnvValue>,
) {
    for (key, value) in pending {
        match original.get(key) {
            Some(original_value) if original_value == value => {}
            _ => {
                ops.push(WorkspaceSaveDiffOp::EnvSet {
                    scope: scope.clone(),
                    key: key.clone(),
                    value: value.clone(),
                });
            }
        }
    }
    for key in original.keys() {
        if !pending.contains_key(key) {
            ops.push(WorkspaceSaveDiffOp::EnvRemove {
                scope: scope.clone(),
                key: key.clone(),
            });
        }
    }
}

fn role_auth_forward_for(
    role: Option<&WorkspaceRoleOverride>,
    agent: Agent,
) -> Option<AuthForwardMode> {
    role.and_then(|r| r.auth_forward_for(agent))
}

fn role_sync_source_dir_for(role: Option<&WorkspaceRoleOverride>, agent: Agent) -> Option<PathBuf> {
    role.and_then(|r| r.sync_source_dir_for(agent))
}

/// Input bundle for a settings-screen save operation.
#[derive(Debug)]
pub struct SettingsSaveInput<'a> {
    pub mounts_original: &'a [jackin_config::GlobalMountRow],
    pub mounts_pending: &'a [jackin_config::GlobalMountRow],
    pub env_original: &'a crate::tui::state::SettingsEnvConfig,
    pub env_pending: &'a crate::tui::state::SettingsEnvConfig,
    pub auth_pending: &'a [crate::tui::state::SettingsAuthRow],
    pub original_github_env: &'a BTreeMap<String, EnvValue>,
    pub github_env: &'a BTreeMap<String, EnvValue>,
    pub trust_pending: &'a [SettingsTrustRow],
    pub git_coauthor_trailer: bool,
    pub git_dco: bool,
}

/// Save all settings tabs and return the reloaded config model.
#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
pub fn save_settings(
    paths: &jackin_core::JackinPaths,
    input: SettingsSaveInput<'_>,
) -> anyhow::Result<AppConfig> {
    AppConfig::validate_global_mount_rows(input.mounts_pending)?;
    validate_settings_env(input.env_pending, input.trust_pending)?;
    let mut editor_doc = jackin_config::ConfigEditor::open(paths)?;

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
            crate::tui::auth::AuthKind::Claude
            | crate::tui::auth::AuthKind::Codex
            | crate::tui::auth::AuthKind::Amp
            | crate::tui::auth::AuthKind::Kimi
            | crate::tui::auth::AuthKind::Opencode
            | crate::tui::auth::AuthKind::Grok => {
                let Some(agent) = crate::tui::auth_config::auth_kind_agent(row.kind) else {
                    continue;
                };
                if !row.kind.supported_modes().contains(&row.mode) {
                    anyhow::bail!(
                        "auth mode {} is not supported for {}",
                        row.mode.as_str(),
                        row.kind.label()
                    );
                }
                let Some(mode) = crate::tui::auth_config::auth_mode_to_auth_forward(row.mode)
                else {
                    anyhow::bail!(
                        "auth mode {} is not supported for {}",
                        row.mode.as_str(),
                        row.kind.label()
                    );
                };
                editor_doc.set_global_auth_forward(agent, mode);
                editor_doc.set_global_sync_source_dir(agent, row.sync_source_dir.as_deref());
            }
            crate::tui::auth::AuthKind::Github => {
                let Some(mode) = crate::tui::auth_config::auth_mode_to_github(row.mode) else {
                    anyhow::bail!(
                        "auth mode {} is not supported for {}",
                        row.mode.as_str(),
                        row.kind.label()
                    );
                };
                editor_doc.set_global_github_auth_forward(mode);
            }
            crate::tui::auth::AuthKind::Zai | crate::tui::auth::AuthKind::Minimax => {}
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

#[cfg(test)]
mod tests;
