// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Active launch preparation, execution, and run.

use crate::runtime::launch::launch_pipeline::launch_phases::{
    InstancePrepared, WorkspaceMaterialized,
};

use jackin_core::CommandRunner;
use jackin_docker::docker_client::DockerApi;

use std::future::Future;
use std::pin::Pin;

use crate::instance::{AdmittedInstance, InstanceStatus};

use super::{
    ActiveLaunch, FinishLaunch, LaunchRuntime, MaterializeImage, MaterializeWorkspace,
    PrepareEnvironment, PrepareInstance, ResolveEnvironment, finish_launch, launch_runtime,
    materialize_image_phase, materialize_workspace_phase, prepare_environment, prepare_instance,
    resolve_environment,
};

pub(crate) async fn prepare_active_launch<D, R, S>(
    launch: &mut ActiveLaunch<'_, D, R>,
    mut sidecar: Pin<&mut S>,
    early_sidecar_result: &mut Option<anyhow::Result<()>>,
    sidecar_required: bool,
) -> anyhow::Result<(InstancePrepared, WorkspaceMaterialized)>
where
    D: DockerApi,
    R: CommandRunner,
    S: Future<Output = anyhow::Result<()>>,
{
    let Some(image_decision) = launch.image_decision.take() else {
        launch.initialized.cleanup.run(launch.docker).await;
        return Err(anyhow::anyhow!("image decision already consumed"));
    };
    let image = materialize_image_phase(
        MaterializeImage {
            paths: launch.paths,
            selector: launch.selector,
            cached_repo: &launch.cached_repo,
            validated_repo: &launch.validated_repo,
            agent: launch.agent,
            supported_agents: &launch.supported_agents,
            rebuild: launch.rebuild,
            opts: launch.opts,
            steps: launch.steps,
            docker: launch.docker,
            runner: launch.runner,
            restoring: launch.restoring,
            container_name: &launch.container_name,
            repo_lock: &mut launch.repo_lock,
            cleanup: &launch.initialized.cleanup,
            classified: launch.initialized.image_phase,
            decision: Some(image_decision),
        },
        sidecar.as_mut(),
        early_sidecar_result,
    )
    .await?;
    let mut prepared = prepare_instance(PrepareInstance {
        paths: launch.paths,
        workspace: launch.workspace,
        workspace_name: &launch.workspace_name,
        container_name: &launch.container_name,
        role_key: &launch.role_key,
        agent_display_name: &launch.agent_display_name,
        agent: launch.agent,
        source: &launch.source,
        opts: launch.opts,
        dind_started: launch.initialized.dind_started,
        dind: &launch.initialized.dind,
        network: &launch.initialized.network,
        certs_volume: &launch.initialized.certs_volume,
        recipe_role_git_sha: launch.recipe_role_git_sha.take(),
        recipe_base_image_ref: launch.recipe_base_image_ref.take(),
        supported_agents: &launch.supported_agents,
        restoring: launch.restoring,
        docker: launch.docker,
        cleanup: &launch.initialized.cleanup,
        image,
    })
    .await?;
    let configured = resolve_environment(ResolveEnvironment {
        config: launch.config,
        opts: launch.opts,
        role_key: &launch.role_key,
        workspace_name: &launch.workspace_name,
        cleanup: &launch.initialized.cleanup,
        docker: launch.docker,
    })
    .await?;
    let trust = prepare_environment(
        PrepareEnvironment {
            paths: launch.paths,
            config: launch.config,
            agent: launch.agent,
            container_name: &launch.container_name,
            validated_repo: &launch.validated_repo,
            role_key: &launch.role_key,
            workspace: launch.workspace,
            steps: launch.steps,
            cleanup: &launch.initialized.cleanup,
            docker: launch.docker,
            configured,
            opts: launch.opts,
        },
        sidecar.as_mut(),
        early_sidecar_result,
    )
    .await?;
    if let Err(error) = launch.account_revision.ensure_current(launch.paths) {
        launch.initialized.cleanup.run(launch.docker).await;
        return Err(error);
    }
    // Record the admitted instances on the manifest now that resolution
    // succeeded, and persist immediately: all downstream paths (docker,
    // detached, apple-container) read the same manifest file.
    prepared
        .instance_manifest
        .set_admitted_instances(trust.instances.iter().map(AdmittedInstance::from));
    if let Err(error) = crate::runtime::launch::write_instance_status(
        launch.paths,
        &prepared.container_state,
        &mut prepared.instance_manifest,
        InstanceStatus::Active,
    ) {
        launch.initialized.cleanup.run(launch.docker).await;
        return Err(error);
    }
    let admitted_instances = trust
        .instances
        .iter()
        .map(AdmittedInstance::from)
        .collect::<Vec<_>>();
    if let Err(error) = crate::runtime::launch::account_identity::record_account_configuration(
        crate::runtime::launch::account_identity::AccountConfigurationRecord {
            root: &trust.environment.state.root,
            paths: launch.paths,
            revision: &launch.account_revision,
            config: launch.config,
            admission_config: &launch.admission_config,
            workspace: trust.environment.workspace_opt.as_ref(),
            role: &launch.role_key,
            admitted: &admitted_instances,
        },
    ) {
        launch.initialized.cleanup.run(launch.docker).await;
        return Err(error);
    }
    let workspace = materialize_workspace_phase(
        MaterializeWorkspace {
            paths: launch.paths,
            config: launch.config,
            selector: launch.selector,
            workspace: launch.workspace,
            docker: launch.docker,
            runner: launch.runner,
            opts: launch.opts,
            steps: launch.steps,
            container_name: &launch.container_name,
            role_key: &launch.role_key,
            agent: launch.agent,
            auth_mode: launch.auth_mode,
            validated_repo: &launch.validated_repo,
            exec_bindings: std::mem::take(&mut launch.exec_bindings),
            git_pull_join: launch.git_pull_join.take(),
            prepared: &mut prepared,
            cleanup: &launch.initialized.cleanup,
            trust,
        },
        sidecar,
        early_sidecar_result.take(),
        sidecar_required,
    )
    .await?;
    Ok((prepared, workspace))
}

pub(crate) async fn execute_active_launch<D, R>(
    launch: ActiveLaunch<'_, D, R>,
    prepared: InstancePrepared,
    workspace_materialized: WorkspaceMaterialized,
) -> anyhow::Result<String>
where
    D: DockerApi,
    R: CommandRunner,
{
    if let Err(error) = launch.account_revision.ensure_current(launch.paths) {
        launch.initialized.cleanup.run(launch.docker).await;
        return Err(error);
    }
    let launched = launch_runtime(LaunchRuntime {
        paths: launch.paths,
        config: launch.config,
        selector: launch.selector,
        workspace: launch.workspace,
        workspace_name: &launch.workspace_name,
        docker: launch.docker,
        runner: launch.runner,
        opts: launch.opts,
        steps: launch.steps,
        container_name: &launch.container_name,
        role_key: &launch.role_key,
        agent_display_name: &launch.agent_display_name,
        agent: launch.agent,
        source: &launch.source,
        backend: launch.backend,
        validated_repo: &launch.validated_repo,
        resolved_env: &launch.resolved_env,
        selected_refresh_reason: launch.selected_refresh_reason,
        git: &launch.git,
        network: &launch.initialized.network,
        dind: &launch.initialized.dind,
        certs_volume: &launch.initialized.certs_volume,
        resolved_profile: launch.initialized.resolved_profile,
        account_revision: launch.account_revision,
        effective_grants: &launch.initialized.effective_grants,
        adopted_sidecar_was_used: launch.initialized.adopted_sidecar_was_used,
        prepared,
        workspace_materialized,
        cleanup: launch.initialized.cleanup,
    })
    .await?;
    finish_launch(FinishLaunch {
        paths: launch.paths,
        config: launch.config,
        workspace_name: &launch.workspace_name,
        docker: launch.docker,
        runner: launch.runner,
        container_name: &launch.container_name,
        launched,
    })
    .await
}

pub(crate) async fn run_active_launch<D, R, S>(
    mut launch: ActiveLaunch<'_, D, R>,
    sidecar: Pin<&mut S>,
    sidecar_required: bool,
) -> anyhow::Result<String>
where
    D: DockerApi,
    R: CommandRunner,
    S: Future<Output = anyhow::Result<()>>,
{
    let mut early_sidecar_result = None;
    let (prepared, workspace) = prepare_active_launch(
        &mut launch,
        sidecar,
        &mut early_sidecar_result,
        sidecar_required,
    )
    .await?;
    execute_active_launch(launch, prepared, workspace).await
}
