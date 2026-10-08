// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Hardline agent start and container reachability gates.
//!
//! The running-state gate lives in
//! `jackin_runtime_attach_running_gate` (S7 split 107),
//! re-exported below.

use jackin_core::{CommandRunner, ContainerHandle};
use jackin_docker::docker_client::DockerApi;

use jackin_core::JackinPaths;

use super::{
    finalize_reconnected_foreground_session_with_handle,
    hardline_docker_agent_with_focus_with_lease,
    start_or_reconnect_capsule_client_with_handle_with_lease, validate_current_account_admission,
    validate_recorded_role_handle,
};

// Moved to jackin_runtime_attach_running_gate::running_gate (S7 split
// 107); the item re-export keeps every
// `attach::require_container_running` path stable.
pub(crate) use jackin_runtime_attach_running_gate::running_gate::require_container_running;

pub(crate) async fn start_or_hardline_agent(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &crate::runtime::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    start_first: bool,
    entry_claim: Option<&crate::runtime::universe::EntryClaim>,
) -> anyhow::Result<()> {
    start_or_hardline_agent_with_known_container(
        paths,
        container_name,
        admission_lease,
        docker,
        runner,
        start_first,
        None,
        entry_claim,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "restore threads exact Docker ownership, account revision, and pending entry leases"
)]
pub(crate) async fn start_or_hardline_agent_with_container_handle(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &crate::runtime::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    start_first: bool,
    container: &ContainerHandle,
    entry_claim: Option<&crate::runtime::universe::EntryClaim>,
) -> anyhow::Result<()> {
    start_or_hardline_agent_with_known_container(
        paths,
        container_name,
        admission_lease,
        docker,
        runner,
        start_first,
        Some(container),
        entry_claim,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "restore threads exact Docker ownership, account revision, and pending entry leases"
)]
pub(crate) async fn start_or_hardline_agent_with_known_container(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &crate::runtime::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    start_first: bool,
    known_container: Option<&ContainerHandle>,
    entry_claim: Option<&crate::runtime::universe::EntryClaim>,
) -> anyhow::Result<()> {
    validate_current_account_admission(paths, container_name, admission_lease)?;
    match crate::runtime::backend::backend_for_state(paths, container_name) {
        crate::runtime::backend::InstanceBackend::Docker => {
            if start_first {
                let container = start_or_reconnect_capsule_client_with_handle_with_lease(
                    paths,
                    container_name,
                    admission_lease,
                    docker,
                    runner,
                    known_container,
                    entry_claim,
                )
                .await?;
                admission_lease.ensure_current(paths)?;
                finalize_reconnected_foreground_session_with_handle(
                    paths,
                    container_name,
                    admission_lease,
                    docker,
                    runner,
                    &container,
                )
                .await
            } else {
                let container = if let Some(container) = known_container {
                    container.clone()
                } else {
                    let inspection = docker.inspect_container_by_name(container_name).await;
                    inspection.handle.ok_or_else(|| {
                        anyhow::anyhow!(
                            "cannot resolve container {container_name}: {}",
                            inspection.state.inspect_label()
                        )
                    })?
                };
                validate_recorded_role_handle(paths, container_name, &container)?;
                hardline_docker_agent_with_focus_with_lease(
                    paths,
                    container_name,
                    None,
                    admission_lease,
                    docker,
                    runner,
                    &container,
                    entry_claim,
                )
                .await
            }
        }
        crate::runtime::backend::InstanceBackend::AppleContainer => {
            anyhow::ensure!(
                known_container.is_none(),
                "immutable Docker container handle supplied for Apple Container backend"
            );
            admission_lease.ensure_current(paths)?;
            crate::runtime::apple_container::reconnect(paths, container_name, None, entry_claim)
                .await?;
            admission_lease.ensure_current(paths)?;
            anyhow::bail!("apple-container finalize not yet implemented - Phase 0")
        }
    }
}

/// Verify the container is reachable (running/paused/restarting).
/// Returns `Ok(())` when reachable, `Err` otherwise.
/// `stopped_hint` is the trailing clause of the "is stopped" error, e.g. "restart it before opening a shell".
pub(crate) async fn require_container_reachable(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    stopped_hint: &str,
) -> anyhow::Result<(
    crate::runtime::launch::AccountConfigRevision,
    ContainerHandle,
)> {
    let container = require_container_running(paths, container_name, docker, stopped_hint).await?;
    let admission_lease = crate::runtime::launch::AccountConfigRevision::acquire(paths)?;
    validate_current_account_admission(paths, container_name, &admission_lease)?;
    Ok((admission_lease, container))
}
