// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace save diff plans and edit building.

use super::{push_auth_forward_diff, push_env_diff, validate_settings_env_keys};
use jackin_config::{EnvScope, EnvValue, GithubAuthMode, WorkspaceConfig, WorkspaceEdit};
use std::collections::BTreeSet;

use crate::tui::screens::settings::model::{SettingsEnvConfig, SettingsTrustRow};
use jackin_core::{Agent, WorkspaceName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSaveDiffOp {
    WorkspaceAccounts {
        accounts: Vec<String>,
    },
    WorkspaceAccountBinding {
        agent: Agent,
        account: Option<String>,
    },
    WorkspaceRoleAccountBinding {
        role: String,
        agent: Agent,
        account: Option<String>,
    },
    WorkspaceGithubAuthForward {
        mode: Option<GithubAuthMode>,
    },
    WorkspaceRoleGithubAuthForward {
        role: String,
        mode: Option<GithubAuthMode>,
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
    if pending.default_launch != original.default_launch {
        edit.default_launch = Some(pending.default_launch.clone());
    }
    edit
}
