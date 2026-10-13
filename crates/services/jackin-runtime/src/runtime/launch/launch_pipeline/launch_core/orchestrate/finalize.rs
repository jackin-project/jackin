// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Session finalization, sidecar polling, and launch failure handling.

use crate::runtime::launch::launch_pipeline::launch_phases::SessionFinalized;

use jackin_core::{CommandRunner, ContainerHandle};
use jackin_docker::docker_client::DockerApi;

use std::future::Future;
use std::pin::Pin;

use crate::instance::{InstanceManifest, InstanceStatus};
use crate::runtime::attach::reconnect_or_create_session_with_container_handle_with_lease;

pub(crate) async fn poll_sidecar_while<T, F, S>(
    work: F,
    mut sidecar: Pin<&mut S>,
    early_sidecar_result: &mut Option<anyhow::Result<()>>,
) -> anyhow::Result<T>
where
    F: Future<Output = anyhow::Result<T>>,
    S: Future<Output = anyhow::Result<()>>,
{
    if early_sidecar_result.is_some() {
        return work.await;
    }

    let mut work = std::pin::pin!(work);
    tokio::select! {
        biased;
        result = sidecar.as_mut() => {
            *early_sidecar_result = Some(result);
            work.await
        }
        result = &mut work => result,
    }
}

pub(crate) struct FinalizeSession<'a, D, R> {
    pub(crate) paths: &'a jackin_core::JackinPaths,
    pub(crate) config: &'a jackin_config::AppConfig,
    pub(crate) workspace_name: &'a Option<String>,
    pub(crate) admission_lease: &'a crate::runtime::launch::account_identity::AccountConfigRevision,
    pub(crate) docker: &'a D,
    pub(crate) runner: &'a mut R,
    pub(crate) container_name: &'a str,
    pub(crate) container: &'a ContainerHandle,
    pub(crate) container_state: &'a std::path::Path,
    pub(crate) instance_manifest: &'a mut InstanceManifest,
    pub(crate) cleanup: &'a mut crate::runtime::launch::LoadCleanup,
}

pub(crate) async fn finalize_session<D, R>(
    input: FinalizeSession<'_, D, R>,
) -> anyhow::Result<SessionFinalized>
where
    D: DockerApi,
    R: CommandRunner,
{
    let FinalizeSession {
        paths,
        config,
        workspace_name,
        admission_lease,
        docker,
        runner,
        container_name,
        container,
        container_state,
        instance_manifest,
        cleanup,
    } = input;
    let finalize_result: anyhow::Result<crate::isolation::finalize::FinalizeDecision> = async {
        crate::runtime::launch::write_instance_status(
            paths,
            container_state,
            instance_manifest,
            InstanceStatus::Running,
        )?;
        let interactive_finalize = true;
        let mut prompt = crate::isolation::finalize::ExitActionPrompt {
            state_dir: paths.data_dir.join(container_name).join("state"),
        };
        let dirty_exit_policy = config.resolve_dirty_exit_policy(
            workspace_name
                .as_deref()
                .and_then(|name| config.workspaces.get(name)),
        );
        admission_lease.ensure_current(paths)?;
        let outcome =
            crate::runtime::launch::inspect_attach_outcome_by_id(docker, container).await?;
        admission_lease.ensure_current(paths)?;
        crate::runtime::launch::write_instance_attach_outcome(
            paths,
            container_state,
            instance_manifest,
            outcome,
        )?;
        let mut decision = crate::isolation::finalize::finalize_foreground_session(
            crate::isolation::finalize::FinalizeContext {
                container_name,
                container_state_dir: &paths.data_dir.join(container_name),
                outcome,
                is_interactive: interactive_finalize,
                dirty_exit_policy,
                prompt: &mut prompt,
                docker,
                runner,
                container: container.clone(),
            },
        )
        .await?;
        admission_lease.ensure_current(paths)?;
        crate::runtime::launch::write_preserved_status_if_applicable(
            decision,
            paths,
            container_state,
            instance_manifest,
        )?;
        if matches!(
            decision,
            crate::isolation::finalize::FinalizeDecision::ReturnToAgent
        ) {
            admission_lease.ensure_current(paths)?;
            reconnect_or_create_session_with_container_handle_with_lease(
                paths,
                container_name,
                None,
                admission_lease,
                docker,
                runner,
                container,
                None,
            )
            .await?;
            admission_lease.ensure_current(paths)?;
            let outcome =
                crate::runtime::launch::inspect_attach_outcome_by_id(docker, container).await?;
            admission_lease.ensure_current(paths)?;
            crate::runtime::launch::write_instance_attach_outcome(
                paths,
                container_state,
                instance_manifest,
                outcome,
            )?;
            decision = crate::isolation::finalize::finalize_foreground_session(
                crate::isolation::finalize::FinalizeContext {
                    container_name,
                    container_state_dir: &paths.data_dir.join(container_name),
                    outcome,
                    is_interactive: interactive_finalize,
                    dirty_exit_policy,
                    prompt: &mut prompt,
                    docker,
                    runner,
                    container: container.clone(),
                },
            )
            .await?;
            admission_lease.ensure_current(paths)?;
            crate::runtime::launch::write_preserved_status_if_applicable(
                decision,
                paths,
                container_state,
                instance_manifest,
            )?;
        }
        Ok(decision)
    }
    .await;
    match finalize_result {
        Ok(decision) => Ok(SessionFinalized { decision }),
        Err(error) => {
            cleanup.run_with_role_handle(docker, container).await;
            Err(error)
        }
    }
}

pub(crate) async fn handle_launch_failure<D: DockerApi>(
    paths: &jackin_core::JackinPaths,
    container_state: &std::path::Path,
    instance_manifest: &mut InstanceManifest,
    container_name: &str,
    cleanup: &crate::runtime::launch::LoadCleanup,
    docker: &D,
) {
    if let Err(status_error) = crate::runtime::launch::write_instance_status(
        paths,
        container_state,
        instance_manifest,
        InstanceStatus::FailedSetup,
    ) && let Some(run) = jackin_diagnostics::active_run()
    {
        run.compact(
            "status",
            &format!(
                "jackin: warning: failed to mark FailedSetup for {container_name} \
                 after launch error: {status_error:#}; on-disk status may be stale"
            ),
        );
    }
    cleanup.run_preserving_evidence(docker).await;
}
