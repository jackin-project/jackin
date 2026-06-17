//! Console-owned config save diff planning.
//!
//! The root binary still applies these operations through `ConfigEditor`, but
//! the rules for what changed between the original and pending console models
//! live with the console crate.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use jackin_config::{
    AuthForwardMode, EnvScope, EnvValue, GithubAuthMode, WorkspaceConfig, WorkspaceEdit,
    WorkspaceRoleOverride,
};
use jackin_core::{Agent, env_model};

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
    workspace_name: &str,
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
    workspace_name: &str,
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) {
    let ws_scope = EnvScope::Workspace(workspace_name.to_owned());
    push_env_map_diff(ops, ws_scope, &original.env, &pending.env);

    let empty = BTreeMap::<String, EnvValue>::new();
    let orig_ws_github_env = original.github.as_ref().map_or(&empty, |g| &g.env);
    let pend_ws_github_env = pending.github.as_ref().map_or(&empty, |g| &g.env);
    let ws_github_scope = EnvScope::WorkspaceGithub(workspace_name.to_owned());
    push_env_map_diff(ops, ws_github_scope, orig_ws_github_env, pend_ws_github_env);

    let role_keys: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    for role in role_keys {
        let orig_env = original.roles.get(role).map_or(&empty, |o| &o.env);
        let pend_env = pending.roles.get(role).map_or(&empty, |p| &p.env);
        let scope = EnvScope::WorkspaceRole {
            workspace: workspace_name.to_owned(),
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
            workspace: workspace_name.to_owned(),
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

#[cfg(test)]
mod tests;
