// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::{
    AppConfig, LoadWorkspaceInput, MountHealReport, ResolvedWorkspace, current_dir_workspace,
};
use jackin_core::RoleSelector;

#[derive(Debug, Clone)]
pub struct WorkspaceChoice {
    pub name: String,
    pub workspace: ResolvedWorkspace,
    pub allowed_roles: Vec<RoleSelector>,
    pub default_role: Option<String>,
    pub last_role: Option<String>,
    pub input: LoadWorkspaceInput,
}

pub fn build_workspace_choice(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: &LoadWorkspaceInput,
) -> anyhow::Result<Option<WorkspaceChoice>> {
    match input {
        LoadWorkspaceInput::CurrentDir => {
            let current = current_dir_workspace(cwd)?;
            Ok(Some(WorkspaceChoice {
                name: "Current directory".to_owned(),
                workspace: ResolvedWorkspace {
                    name: current.workdir.clone(),
                    label: current.workdir.clone(),
                    workdir: current.workdir,
                    mounts: current.mounts,
                    default_agent: None,
                    keep_awake_enabled: false,
                    git_pull_on_entry: false,
                    mount_heal: MountHealReport::default(),
                },
                allowed_roles: crate::workspace::configured_roles(config.roles.keys()),
                default_role: None,
                last_role: None,
                input: LoadWorkspaceInput::CurrentDir,
            }))
        }
        LoadWorkspaceInput::Saved(name) => {
            let Some(saved) = config.workspaces.get(name) else {
                return Ok(None);
            };
            let allowed_roles =
                crate::workspace::eligible_roles_for_workspace(config.roles.keys(), saved);
            Ok(Some(WorkspaceChoice {
                name: name.clone(),
                workspace: ResolvedWorkspace {
                    name: name.clone(),
                    label: name.clone(),
                    workdir: saved.workdir.clone(),
                    mounts: saved.mounts.clone(),
                    default_agent: saved.default_agent,
                    keep_awake_enabled: saved.keep_awake.enabled,
                    git_pull_on_entry: saved.git_pull_on_entry,
                    mount_heal: MountHealReport::default(),
                },
                allowed_roles,
                default_role: saved.default_role.clone(),
                last_role: saved.last_role.clone(),
                input: LoadWorkspaceInput::Saved(name.clone()),
            }))
        }

        LoadWorkspaceInput::Path { .. } => Ok(None),
    }
}
