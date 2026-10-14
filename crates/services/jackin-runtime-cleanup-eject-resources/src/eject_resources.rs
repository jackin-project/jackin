// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ContainerHandle, JackinPaths};
use jackin_docker::docker_client::DockerApi;
use jackin_instance::DockerResources;
use jackin_runtime_cleanup_socket_dir::socket_dir::remove_socket_dir;

pub async fn eject_docker_role_with_resources(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    role_handle: &ContainerHandle,
    dind_handle: Option<&ContainerHandle>,
    resources: &DockerResources,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        role_handle.name() == container_name,
        "role container handle name mismatch: expected {container_name}, got {}",
        role_handle.name()
    );
    if let Some(dind_container) = resources.dind_container.as_deref() {
        let Some(dind_handle) = dind_handle else {
            anyhow::bail!(
                "DinD container identity unavailable; aborting destructive cleanup for {dind_container}"
            );
        };
        anyhow::ensure!(
            dind_handle.name() == dind_container,
            "DinD container handle name mismatch: expected {dind_container}, got {}",
            dind_handle.name()
        );
    }

    docker.remove_container_by_id(role_handle).await?;
    if resources.dind_container.is_some() {
        let dind_handle = dind_handle.ok_or_else(|| {
            anyhow::anyhow!("DinD container identity disappeared before destructive cleanup")
        })?;
        docker.remove_container_by_id(dind_handle).await?;
    }

    if let Some(certs_volume) = resources.certs_volume.as_deref() {
        docker.remove_volume(certs_volume).await?;
    }
    docker.remove_network(&resources.network).await?;

    remove_socket_dir(paths, container_name).await;

    Ok(())
}
