// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Restore-container reconnect with container-handle leases.

use anyhow::Context as _;

use jackin_core::{CommandRunner, ContainerHandle};
use jackin_docker::docker_client::DockerApi;

use jackin_core::JackinPaths;

use super::{
    ContainerState, inspect_unavailable_message, missing_restore_message,
    reconnect_or_create_session_with_container_handle_with_lease,
    validate_current_account_admission, validate_recorded_role_handle,
};

pub(crate) async fn inspect_restore_container(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    known_container: Option<&ContainerHandle>,
) -> anyhow::Result<(ContainerState, Option<ContainerHandle>)> {
    let (inspect, inspect_handle) = if let Some(container) = known_container {
        validate_recorded_role_handle(paths, container_name, container)?;
        let current =
            crate::runtime::cleanup::resolve_role_handle_for_state(paths, container_name, docker)
                .await?;
        anyhow::ensure!(
            current == *container,
            "Docker ownership identity changed for {container_name}"
        );
        (
            docker.inspect_container_by_id(container).await,
            Some(container.clone()),
        )
    } else {
        let inspection = docker.inspect_container_by_name(container_name).await;
        (inspection.state, inspection.handle)
    };
    if let Some(container) = &inspect_handle {
        validate_recorded_role_handle(paths, container_name, container)?;
    }
    Ok((inspect, inspect_handle))
}

pub(crate) async fn start_or_reconnect_capsule_client_with_handle_with_lease(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &crate::runtime::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    known_container: Option<&ContainerHandle>,
    mut entry_claim: Option<&crate::runtime::universe::EntryClaim>,
) -> anyhow::Result<ContainerHandle> {
    validate_current_account_admission(paths, container_name, admission_lease)?;
    if let Some(container) = known_container {
        anyhow::ensure!(
            container.name() == container_name,
            "container handle name mismatch: expected {container_name}, got {}",
            container.name()
        );
    }
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "restore_inspect",
        Some(container_name),
    );
    let (inspect, inspect_handle) =
        inspect_restore_container(paths, container_name, docker, known_container).await?;
    let inspect_label = inspect.short_label();
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "restore_inspect",
        Some(&inspect_label),
    );
    match inspect {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
            if inspect_handle.is_none() {
                anyhow::bail!("container '{container_name}' inspection returned no immutable ID");
            }
        }
        ContainerState::Stopped { .. } | ContainerState::Created => {
            let Some(container) = inspect_handle.clone() else {
                anyhow::bail!("container '{container_name}' inspection returned no immutable ID");
            };
            let resources =
                crate::runtime::cleanup::docker_resources_for_state(paths, container_name)?;
            restart_stopped_dind_if_needed(paths, container_name, admission_lease, docker).await?;

            jackin_diagnostics::active_timing_started(
                jackin_diagnostics::DiagnosticStage::Capsule,
                "restore_start_container",
                Some(container_name),
            );
            admission_lease.ensure_current(paths)?;
            let start_result = docker
                .start_container_by_id(&container)
                .await
                .with_context(|| format!("starting role container {container_name}"));
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Capsule,
                "restore_start_container",
                if start_result.is_ok() {
                    Some("started")
                } else {
                    Some("error")
                },
            );
            if let Err(start_err) = start_result {
                crate::runtime::launch::ensure_current_or_remove_stale_container(
                    admission_lease,
                    paths,
                    &container,
                    docker,
                )
                .await?;
                let net_missing = if let Ok(None) = docker.inspect_network(&resources.network).await
                {
                    true
                } else {
                    let err_msg = start_err.to_string();
                    err_msg.contains("network")
                        && (err_msg.contains("not found") || err_msg.contains("404"))
                };
                crate::runtime::launch::ensure_current_or_remove_stale_container(
                    admission_lease,
                    paths,
                    &container,
                    docker,
                )
                .await?;
                if net_missing {
                    anyhow::bail!(
                        "role container '{container_name}' cannot be started because its Docker network '{}' no longer exists; \
                         run `jackin load` to recreate the instance, or `jackin eject {container_name}` to discard it",
                        resources.network
                    );
                }
                return Err(start_err);
            }
            if let Some(claim) = entry_claim.take() {
                claim
                    .activate()
                    .await
                    .context("activating running launch entry")?;
            }
            crate::runtime::launch::ensure_current_or_remove_stale_container(
                admission_lease,
                paths,
                &container,
                docker,
            )
            .await?;
        }
        ContainerState::NotFound => {
            if let Some(message) = missing_restore_message(paths, container_name)? {
                anyhow::bail!("{message}");
            }
            anyhow::bail!(
                "container '{container_name}' not found; use `jackin load` to start a new session"
            );
        }
        ContainerState::InspectUnavailable(reason) => {
            anyhow::bail!("{}", inspect_unavailable_message(container_name, &reason));
        }
        state @ (ContainerState::Removing | ContainerState::Dead) => {
            anyhow::bail!(
                "container '{container_name}' is not startable (state: {}); \
                 use `jackin load` to start a new session",
                state.short_label()
            );
        }
    }
    jackin_host::caffeinate::reconcile(paths, docker, runner).await;
    let Some(container) = inspect_handle else {
        anyhow::bail!("container '{container_name}' has no immutable ID for attach");
    };
    reconnect_or_create_session_with_container_handle_with_lease(
        paths,
        container_name,
        None,
        admission_lease,
        docker,
        runner,
        &container,
        entry_claim,
    )
    .await?;
    Ok(container)
}

pub(crate) async fn restart_stopped_dind_if_needed(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &crate::runtime::launch::AccountConfigRevision,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let resources = crate::runtime::cleanup::docker_resources_for_state(paths, container_name)?;
    if resources.dind_container.is_none() {
        return Ok(());
    }
    let Some(dind) =
        crate::runtime::cleanup::resolve_dind_handle_for_state(paths, container_name, docker)
            .await?
    else {
        return Ok(());
    };
    let dind_state = docker.inspect_container_by_id(&dind).await;
    if !matches!(
        dind_state,
        ContainerState::Stopped { .. } | ContainerState::Created
    ) {
        return Ok(());
    }
    admission_lease.ensure_current(paths)?;
    drop(docker.start_container_by_id(&dind).await);
    crate::runtime::launch::ensure_current_or_remove_stale_container(
        admission_lease,
        paths,
        &dind,
        docker,
    )
    .await
}
