// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Env and auth-forward diff pushers.

use super::WorkspaceSaveDiffOp;
use jackin_config::{EnvScope, EnvValue, WorkspaceConfig};
use std::collections::{BTreeMap, BTreeSet};

use jackin_core::{Agent, WorkspaceName, is_account_env, is_reserved};

pub(crate) fn validate_settings_env_keys<'a>(
    scope: &str,
    keys: impl Iterator<Item = &'a String> + 'a,
) -> anyhow::Result<()> {
    for key in keys {
        if key.trim().is_empty() {
            anyhow::bail!("env var key cannot be empty");
        }
        if is_reserved(key) {
            anyhow::bail!(
                "env name {key:?} in {scope} is reserved by the jackin runtime and cannot be set"
            );
        }
        if is_account_env(key) {
            anyhow::bail!(
                "env name {key:?} in {scope} belongs to account credentials and cannot be set here"
            );
        }
    }
    Ok(())
}

pub(crate) fn push_auth_forward_diff(
    ops: &mut Vec<WorkspaceSaveDiffOp>,
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) {
    if original.accounts != pending.accounts {
        ops.push(WorkspaceSaveDiffOp::WorkspaceAccounts {
            accounts: pending.accounts.clone(),
        });
    }
    for agent in Agent::ALL {
        if original.account_bindings.get(agent) != pending.account_bindings.get(agent) {
            ops.push(WorkspaceSaveDiffOp::WorkspaceAccountBinding {
                agent: *agent,
                account: pending.account_bindings.get(agent).cloned(),
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
        for agent in Agent::ALL {
            let before = orig_override.and_then(|r| r.account_bindings.get(agent));
            let after = pend_override.and_then(|r| r.account_bindings.get(agent));
            if before != after {
                ops.push(WorkspaceSaveDiffOp::WorkspaceRoleAccountBinding {
                    role: role.clone(),
                    agent: *agent,
                    account: after.cloned(),
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

pub(crate) fn push_env_diff(
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

pub(crate) fn push_env_map_diff(
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
