// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `run_launch_phases` phase chain entry.

use crate::runtime::launch::launch_pipeline::launch_core::LaunchCore;

use jackin_core::CommandRunner;
use jackin_docker::docker_client::DockerApi;

use super::{ActiveLaunch, InitializeLaunch, initialize_launch, run_active_launch};

pub(crate) async fn run_launch_phases<D, R>(ctx: LaunchCore<'_, D, R>) -> anyhow::Result<String>
where
    D: DockerApi,
    R: CommandRunner,
{
    // Destructure captured names so verbatim original block statements work.
    let LaunchCore {
        paths,
        config,
        selector,
        workspace,
        docker,
        runner,
        opts,
        git,
        workspace_name,
        steps,
        role_key,
        agent_display_name,
        agent,
        supported_agents,
        cached_repo,
        validated_repo,
        source,
        auth_mode,
        backend,
        image_decision,
        repo_lock,
        restoring,
        container_name,
        exec_bindings,
        recipe_role_git_sha,
        recipe_base_image_ref,
        selected_refresh_reason,
        resolved_env,
        rebuild,
        restore_pinned_sha: _,
        git_pull_join,
        account_revision,
        admission_config,
        ..
    } = ctx;
    let initialized = initialize_launch(InitializeLaunch {
        paths,
        config,
        selector,
        workspace,
        docker,
        opts,
        validated_repo: &validated_repo,
        image_decision: &image_decision,
        container_name: &container_name,
    })
    .await?;
    let launch = ActiveLaunch {
        paths,
        config,
        selector,
        workspace,
        docker,
        runner,
        opts,
        git,
        workspace_name,
        steps,
        role_key,
        agent_display_name,
        agent,
        supported_agents,
        cached_repo,
        validated_repo,
        source,
        auth_mode,
        backend,
        image_decision: Some(image_decision),
        repo_lock,
        restoring,
        container_name,
        exec_bindings,
        recipe_role_git_sha,
        recipe_base_image_ref,
        selected_refresh_reason,
        resolved_env,
        rebuild,
        git_pull_join,
        account_revision,
        admission_config,
        initialized,
    };
    // Start the sidecar future before image materialization so network/DinD
    // setup can make progress while runtime binaries and Docker build run.
    launch.steps.stage_started(
        crate::runtime::progress::LaunchStage::Network,
        "wiring private network",
    );
    let sidecar_container = launch.container_name.clone();
    let sidecar_network = launch.initialized.network.clone();
    let sidecar_dind = launch.initialized.dind.clone();
    let sidecar_certs_volume = launch.initialized.certs_volume.clone();
    let sidecar_dind_grant = launch.initialized.effective_grants.dind;
    let sidecar_dind_handle_slot = launch.initialized.cleanup.dind_handle_slot();
    let sidecar_network_disabled =
        crate::runtime::docker_profile::network_disabled(&launch.initialized.effective_grants);
    let role_network_internal = crate::runtime::docker_profile::role_network_internal(
        launch.initialized.resolved_profile.0,
    );
    let adopted_sidecar_was_used = launch.initialized.adopted_sidecar_was_used;
    let dind_started = launch.initialized.dind_started;
    let sidecar_required = adopted_sidecar_was_used || dind_started;
    if sidecar_required {
        launch.steps.stage_started(
            crate::runtime::progress::LaunchStage::Sidecar,
            "starting sidecar",
        );
    } else {
        launch.steps.stage_skipped(
            crate::runtime::progress::LaunchStage::Sidecar,
            "sidecar not required",
        );
    }
    let docker = launch.docker;
    let sidecar = async move {
        if adopted_sidecar_was_used {
            Ok(())
        } else if dind_started {
            crate::runtime::launch::run_dind_sidecar_headless(
                &sidecar_container,
                &sidecar_network,
                &sidecar_dind,
                &sidecar_certs_volume,
                sidecar_dind_grant,
                sidecar_dind_handle_slot,
                docker,
            )
            .await
        } else if sidecar_network_disabled {
            Ok(())
        } else {
            crate::runtime::launch::create_role_network(
                &sidecar_container,
                &sidecar_network,
                role_network_internal,
                docker,
            )
            .await
        }
    };
    let mut sidecar = std::pin::pin!(sidecar);
    run_active_launch(launch, sidecar.as_mut(), sidecar_required).await
}
