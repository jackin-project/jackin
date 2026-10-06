// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `MaterializeWorkspace` input and workspace materialization.

use crate::runtime::launch::launch_pipeline::launch_phases::{
    InstancePrepared, TrustSeeded, WorkspaceMaterialized,
};

use super::helpers::{emit_auth_breadcrumbs, workspace_launch_config};
use jackin_core::CommandRunner;
use jackin_docker::docker_client::DockerApi;

use std::future::Future;
use std::pin::Pin;

pub(crate) struct MaterializeWorkspace<'a, D, R> {
    pub(crate) paths: &'a jackin_core::JackinPaths,
    pub(crate) config: &'a jackin_config::AppConfig,
    pub(crate) selector: &'a jackin_core::RoleSelector,
    pub(crate) workspace: &'a jackin_config::ResolvedWorkspace,
    pub(crate) docker: &'a D,
    pub(crate) runner: &'a mut R,
    pub(crate) opts: &'a crate::runtime::launch::LoadOptions,
    pub(crate) steps: &'a mut crate::runtime::launch::StepCounter,
    pub(crate) container_name: &'a str,
    pub(crate) role_key: &'a str,
    pub(crate) agent: jackin_core::Agent,
    pub(crate) auth_mode: jackin_config::AuthForwardMode,
    pub(crate) validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    pub(crate) exec_bindings: Vec<jackin_protocol::ExecBinding>,
    pub(crate) git_pull_join: Option<crate::runtime::launch::launch_pipeline::DeferredGitPull>,
    pub(crate) prepared: &'a mut InstancePrepared,
    pub(crate) cleanup: &'a crate::runtime::launch::LoadCleanup,
    pub(crate) trust: TrustSeeded,
}

pub(crate) async fn materialize_workspace_phase<D, R, S>(
    input: MaterializeWorkspace<'_, D, R>,
    mut sidecar: Pin<&mut S>,
    early_sidecar_result: Option<anyhow::Result<()>>,
    sidecar_required: bool,
) -> anyhow::Result<WorkspaceMaterialized>
where
    D: DockerApi,
    R: CommandRunner,
    S: Future<Output = anyhow::Result<()>>,
{
    let MaterializeWorkspace {
        paths,
        config,
        selector,
        workspace,
        docker,
        runner,
        opts,
        steps,
        container_name,
        role_key,
        agent,
        auth_mode,
        validated_repo,
        exec_bindings,
        git_pull_join,
        prepared,
        cleanup,
        trust: TrustSeeded { environment, .. },
    } = input;
    emit_auth_breadcrumbs(
        agent,
        auth_mode,
        environment.github_mode,
        &environment.github_env_decls,
    );
    let workspace_label = workspace
        .as_workspace_label()
        .map_err(anyhow::Error::from)?;
    if let Some(git_pull_join) = git_pull_join {
        crate::runtime::launch::launch_pipeline::finish_deferred_git_pull(git_pull_join, steps)
            .await?;
    }
    steps.stage_started(
        crate::runtime::progress::LaunchStage::Workspace,
        "materializing workspace",
    );
    let preflight = crate::isolation::materialize::PreflightContext {
        workspace_label: workspace_label.clone(),
        force: opts.force,
        interactive: true,
    };
    let materialize = crate::isolation::materialize::materialize_workspace(
        workspace,
        &prepared.container_state,
        role_key,
        container_name,
        environment.workspace_opt.as_ref(),
        &preflight,
        runner,
    );
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Workspace,
        "materialize_workspace",
        None,
    );
    let materialize_wait = async {
        if let Some(progress) = steps.progress_mut() {
            progress.while_waiting(materialize).await
        } else {
            materialize.await
        }
    };
    let sidecar_wait = async {
        if let Some(result) = early_sidecar_result {
            result
        } else {
            sidecar.as_mut().await
        }
    };
    let (sidecar_result, materialize_result) = tokio::join!(sidecar_wait, materialize_wait);
    steps.stage_done(crate::runtime::progress::LaunchStage::Network, "isolated");
    match &sidecar_result {
        Ok(()) if sidecar_required => {
            steps.stage_done(crate::runtime::progress::LaunchStage::Sidecar, "ready");
        }
        Err(_) if sidecar_required => {
            steps.stage_error(crate::runtime::progress::LaunchStage::Sidecar);
        }
        _ => {}
    }
    if let Err(error) = sidecar_result {
        crate::runtime::launch::launch_pipeline::launch_phases::mark_failed_setup_then_cleanup(
            paths,
            &prepared.container_state,
            container_name,
            &mut prepared.instance_manifest,
            cleanup,
            docker,
            "sidecar error",
        )
        .await;
        return Err(error);
    }
    let materialized = match materialize_result {
        Ok(materialized) => materialized,
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Workspace,
                "materialize_workspace",
                Some("error"),
            );
            crate::runtime::launch::launch_pipeline::launch_phases::mark_failed_setup_then_cleanup(
                paths,
                &prepared.container_state,
                container_name,
                &mut prepared.instance_manifest,
                cleanup,
                docker,
                "workspace materialization error",
            )
            .await;
            return Err(error);
        }
    };
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Workspace,
        "materialize_workspace",
        Some("materialized"),
    );
    steps.stage_done(
        crate::runtime::progress::LaunchStage::Workspace,
        "materialized",
    );
    let dirty_exit_policy =
        config.resolve_dirty_exit_policy(config.workspaces.get(workspace_label.as_str()));
    let mut launch_config = workspace_launch_config(
        config,
        selector,
        workspace,
        environment.workspace_opt.as_ref(),
        role_key,
        agent,
        validated_repo,
        opts,
        &materialized,
        dirty_exit_policy.as_str(),
        exec_bindings,
        &environment.state,
    )?;
    crate::usage_relay::populate_launch_usage_capabilities(config, &mut launch_config);
    Ok(WorkspaceMaterialized {
        materialized,
        launch_config,
        environment,
    })
}
