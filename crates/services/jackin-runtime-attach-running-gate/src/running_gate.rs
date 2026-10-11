// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Docker lifecycle gate shared by the attach spawn paths.
//!
//! [`require_container_running`] inspects the container and returns
//! the prevalidated handle when the lifecycle state is reachable
//! (running, paused, or restarting). Every other state bails with
//! the spawn-facing message: the restore hint for a missing
//! container, the transport reason when inspection is unavailable,
//! and the hardline hint for a stopped container. Admission
//! (account revision, instance policy) stays with the callers.

use jackin_core::{ContainerHandle, JackinPaths};
use jackin_docker::docker_client::{ContainerState, DockerApi};
use jackin_runtime_attach_admission::admission::validate_recorded_role_handle;
use jackin_runtime_attach_inspect::inspect::missing_restore_message;
use jackin_runtime_attach_sessions::sessions::inspect_unavailable_message;

/// Verify only the Docker lifecycle state. New-agent sessions call this
/// before their single fresh manifest/policy admission gate so that target
/// selection and revalidation remain one pre-exec decision.
pub async fn require_container_running(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    stopped_hint: &str,
) -> anyhow::Result<ContainerHandle> {
    let inspection = docker.inspect_container_by_name(container_name).await;
    match inspection.state {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
            let container = inspection.handle.ok_or_else(|| {
                anyhow::anyhow!("container '{container_name}' inspection returned no immutable ID")
            })?;
            validate_recorded_role_handle(paths, container_name, &container)?;
            Ok(container)
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
        ContainerState::Stopped { .. }
        | ContainerState::Created
        | ContainerState::Removing
        | ContainerState::Dead => {
            anyhow::bail!(
                "container '{container_name}' is stopped; run `jackin hardline {container_name}` to {stopped_hint}"
            );
        }
    }
}
