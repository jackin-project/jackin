// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use jackin_config::AppConfig;
use jackin_core::{JackinPaths, RoleSelector, WorkspaceName};
use jackin_docker::docker_client::DockerApi;

pub(super) struct AccountAdmission {
    pub(super) initial_account_revision: super::super::account_identity::AccountConfigRevision,
    pub(super) selected_workspace: Option<WorkspaceName>,
    pub(super) admission_config: AppConfig,
    pub(super) account_config: Option<AppConfig>,
}

pub(super) async fn prepare(
    paths: &JackinPaths,
    config: &AppConfig,
    selector: &RoleSelector,
    workspace: &jackin_config::ResolvedWorkspace,
    docker: &impl DockerApi,
    opts: &super::super::LoadOptions,
) -> anyhow::Result<AccountAdmission> {
    let initial_account_revision =
        super::super::account_identity::AccountConfigRevision::acquire_bound(paths, config)?;
    let selected_workspace = config
        .workspaces
        .contains_key(workspace.name.as_str())
        .then(|| WorkspaceName::parse(&workspace.name))
        .transpose()?;
    if super::absent_container_can_apply_model_and_effort(docker, opts).await? {
        super::validate_absent_rebuild_restore_options(paths, opts)?;
    } else {
        super::validate_explicit_restore_options(paths, opts)?;
    }
    let admission_config = config.clone();
    let account_config = opts
        .selection
        .as_ref()
        .map(|selection| {
            let agent = opts.agent.or(workspace.default_agent).ok_or_else(|| {
                anyhow::anyhow!("select an agent when selecting an account or configuration")
            })?;
            match selection {
                jackin_core::LaunchSelection::Account(id) => {
                    super::super::programmatic::with_account_selection(
                        config,
                        agent,
                        selected_workspace.as_ref(),
                        &selector.key(),
                        id,
                    )
                }
                jackin_core::LaunchSelection::Configuration(id) => {
                    super::super::programmatic::with_configuration_selection(
                        config,
                        agent,
                        selected_workspace.as_ref(),
                        &selector.key(),
                        id,
                    )
                }
            }
        })
        .transpose()?;
    Ok(AccountAdmission {
        initial_account_revision,
        selected_workspace,
        admission_config,
        account_config,
    })
}
