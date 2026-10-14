// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    AccountChoice, WorkspaceChoice, accounts_for_launch, admitted_account_choices,
    build_workspace_choice,
};
use jackin_config::{AppConfig, LoadWorkspaceInput, ResolvedWorkspace, resolve_load_workspace};
use jackin_core::{Agent, RoleSelector, WorkspaceName};

#[derive(Debug)]
pub struct CommittedRoleLaunch {
    pub input: LoadWorkspaceInput,
    pub workspace: ResolvedWorkspace,
}

pub fn resolve_committed_role_launch(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
    role: &RoleSelector,
) -> anyhow::Result<Option<CommittedRoleLaunch>> {
    let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let workspace = resolve_selected_workspace(config, cwd, &choice, role)?;
    Ok(Some(CommittedRoleLaunch { input, workspace }))
}

pub fn resolve_account_launch_workspace(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: &LoadWorkspaceInput,
    selector: &RoleSelector,
) -> anyhow::Result<Option<ResolvedWorkspace>> {
    let Some(choice) = build_workspace_choice(config, cwd, input)? else {
        return Ok(None);
    };
    resolve_selected_workspace(config, cwd, &choice, selector).map(Some)
}

pub(crate) fn resolve_selected_workspace(
    config: &AppConfig,
    cwd: &std::path::Path,
    choice: &WorkspaceChoice,
    role: &RoleSelector,
) -> anyhow::Result<ResolvedWorkspace> {
    Ok(resolve_load_workspace(
        config,
        role,
        cwd,
        choice.input.clone(),
        &[],
    )?)
}

#[derive(Debug)]
pub struct CommittedAgentLaunch {
    pub input: LoadWorkspaceInput,
    pub role: RoleSelector,
    pub workspace: ResolvedWorkspace,
    pub accounts: Vec<AccountChoice>,
}

pub fn resolve_committed_agent_launch(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
    role: RoleSelector,
    agent: Agent,
) -> anyhow::Result<Option<CommittedAgentLaunch>> {
    let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let workspace = resolve_selected_workspace(config, cwd, &choice, &role)?;
    let workspace_name = match &input {
        LoadWorkspaceInput::Saved(name) => Some(WorkspaceName::parse(name)?),
        LoadWorkspaceInput::CurrentDir | LoadWorkspaceInput::Path { .. } => None,
    };
    let accounts =
        match admitted_account_choices(config, workspace_name.as_ref(), &role.key(), agent)? {
            Some(admitted) => admitted,
            None => accounts_for_launch(config, workspace_name.as_ref(), agent),
        };
    Ok(Some(CommittedAgentLaunch {
        input,
        role,
        workspace,
        accounts,
    }))
}
