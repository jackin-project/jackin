// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ContainerHandle, JackinPaths};
use jackin_docker::docker_client::{ContainerState, DockerApi};
use jackin_runtime_attach_admission::admission::validate_recorded_role_handle;
use jackin_runtime_attach_inspect::inspect::missing_restore_message;
use jackin_runtime_attach_sessions::sessions::inspect_unavailable_message;

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
