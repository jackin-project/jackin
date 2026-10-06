// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ClassifyCleanup` input and cleanup classification.

use crate::runtime::launch::launch_pipeline::launch_phases::{CleanupClassified, SessionFinalized};
use crate::runtime::launch::launch_pipeline::purge_or_mark_clean_exited;

use jackin_core::{CommandRunner, ContainerHandle};
use jackin_docker::docker_client::DockerApi;

use crate::instance::{InstanceManifest, InstanceStatus};
use crate::runtime::attach::{AgentSessionInventory, ContainerState, inspect_agent_sessions};

pub(crate) struct ClassifyCleanup<'a, D, R> {
    pub(crate) paths: &'a jackin_core::JackinPaths,
    pub(crate) docker: &'a D,
    pub(crate) runner: &'a mut R,
    pub(crate) container_name: &'a str,
    pub(crate) container_state: &'a std::path::Path,
    pub(crate) instance_manifest: &'a mut InstanceManifest,
    pub(crate) cleanup: &'a mut crate::runtime::launch::LoadCleanup,
    pub(crate) finalized: SessionFinalized,
    pub(crate) container: &'a ContainerHandle,
}

pub(crate) async fn classify_cleanup<D, R>(
    input: ClassifyCleanup<'_, D, R>,
) -> anyhow::Result<CleanupClassified>
where
    D: DockerApi,
    R: CommandRunner,
{
    let ClassifyCleanup {
        paths,
        docker,
        runner,
        container_name,
        container_state,
        instance_manifest,
        cleanup,
        finalized: SessionFinalized { decision },
        container,
    } = input;
    let is_preserved = matches!(
        decision,
        crate::isolation::finalize::FinalizeDecision::Preserved
    );
    let teardown_result: anyhow::Result<()> = async {
        let inspected_state = docker.inspect_container_by_id(container).await;
        match inspected_state {
            ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
                if is_preserved {
                    let sessions =
                        inspect_agent_sessions(docker, container, &ContainerState::Running).await;
                    if let AgentSessionInventory::Unavailable(_) = sessions {
                        let _warning = jackin_telemetry::record_recovered_degradation();
                    }
                    if matches!(&sessions, AgentSessionInventory::Sessions(v) if v.is_empty()) {
                        crate::runtime::launch::write_instance_status(
                            paths,
                            container_state,
                            instance_manifest,
                            InstanceStatus::CleanExited,
                        )?;
                        cleanup.run_with_role_handle(docker, container).await;
                    } else {
                        cleanup.disarm();
                    }
                } else {
                    crate::runtime::launch::write_instance_status(
                        paths,
                        container_state,
                        instance_manifest,
                        InstanceStatus::CleanExited,
                    )?;
                    cleanup.run_with_role_handle(docker, container).await;
                }
            }
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            } if is_preserved => cleanup.run_with_role_handle(docker, container).await,
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            } => {
                cleanup.run_with_role_handle(docker, container).await;
                purge_or_mark_clean_exited(
                    paths,
                    container_name,
                    container_state,
                    instance_manifest,
                    docker,
                    runner,
                )
                .await?;
            }
            ContainerState::Stopped { .. }
            | ContainerState::Created
            | ContainerState::Removing
            | ContainerState::Dead => {
                crate::runtime::launch::write_instance_status(
                    paths,
                    container_state,
                    instance_manifest,
                    InstanceStatus::Crashed,
                )?;
                cleanup.run_with_role_handle(docker, container).await;
            }
            ContainerState::InspectUnavailable(reason) => {
                cleanup.disarm();
                anyhow::bail!(
                    "{}",
                    crate::runtime::attach::docker_unavailable_msg(
                        &format!("inspect container `{container_name}` after the session"),
                        &reason,
                    )
                );
            }
            ContainerState::NotFound if is_preserved => {
                cleanup.run_with_role_handle(docker, container).await;
            }
            ContainerState::NotFound => {
                cleanup.run_with_role_handle(docker, container).await;
                purge_or_mark_clean_exited(
                    paths,
                    container_name,
                    container_state,
                    instance_manifest,
                    docker,
                    runner,
                )
                .await?;
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = teardown_result {
        cleanup.run_with_role_handle(docker, container).await;
        return Err(error);
    }
    Ok(CleanupClassified {
        container_name: container_name.to_owned(),
    })
}
