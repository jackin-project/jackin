// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Role ejection with docker resources.

use crate::instance::{DockerResources, InstanceManifest};
use jackin_core::JackinPaths;

use jackin_core::ContainerHandle;
use jackin_docker::docker_client::DockerApi;

use crate::runtime::backend::{ContainerBackend as _, InstanceBackend};

use super::{cleanup_timing, remove_socket_dir, resolve_cleanup_handles_for_state};

pub async fn eject_role(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let _timing = cleanup_timing("eject_role");
    match crate::runtime::backend::backend_for_state(paths, container_name) {
        InstanceBackend::Docker => eject_docker_role(paths, container_name, docker).await,
        InstanceBackend::AppleContainer => {
            crate::runtime::backend::AppleContainerBackend::production()
                .eject(paths, container_name)
                .await
        }
    }
}

pub(crate) async fn eject_docker_role(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let (resources, role, dind) =
        resolve_cleanup_handles_for_state(paths, container_name, None, docker).await?;
    let role = role.ok_or_else(|| anyhow::anyhow!("container {container_name} is not present"))?;
    eject_docker_role_with_resources(
        paths,
        container_name,
        docker,
        &role,
        dind.as_ref(),
        &resources,
    )
    .await
}

/// Eject a Docker role using identities captured before any destructive
/// operation. A persisted sidecar name is lookup context only: if its
/// immutable identity is unavailable, this function aborts before removing
/// either container rather than risking a same-name replacement.
pub(crate) async fn eject_docker_role_with_handles(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    role_handle: &ContainerHandle,
    dind_handle: Option<&ContainerHandle>,
) -> anyhow::Result<()> {
    // Persisted state must authorize reconnect/eject handles too. Handles
    // supplied by an in-flight launch without a manifest remain captured
    // creation identities, rather than a restart name lookup.
    let manifest = InstanceManifest::read_optional(&paths.data_dir.join(container_name))?;
    if let Some(manifest) = manifest.as_ref() {
        let identity = manifest.docker_identity.as_ref().ok_or_else(|| anyhow::anyhow!(
            "Docker ownership identity unavailable for {container_name}; refusing destructive cleanup"
        ))?;
        anyhow::ensure!(
            role_handle.id() == identity.role_container_id,
            "role container ownership identity mismatch for {container_name}"
        );
        if let Some(handle) = dind_handle {
            anyhow::ensure!(
                Some(handle.id()) == identity.dind_container_id.as_deref(),
                "DinD container ownership identity mismatch for {}",
                handle.name()
            );
        }
    }
    let resources = manifest.map_or_else(
        || DockerResources::from_container_name(container_name),
        |manifest| manifest.docker,
    );
    eject_docker_role_with_resources(
        paths,
        container_name,
        docker,
        role_handle,
        dind_handle,
        &resources,
    )
    .await
}

pub(crate) async fn eject_docker_role_with_resources(
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

    // Remove containers first so the network has no active endpoints.
    docker.remove_container_by_id(role_handle).await?;
    if resources.dind_container.is_some() {
        // The prevalidated handle is the only permitted destructive target.
        let dind_handle = dind_handle.ok_or_else(|| {
            anyhow::anyhow!("DinD container identity disappeared before destructive cleanup")
        })?;
        docker.remove_container_by_id(dind_handle).await?;
    }

    // Volume and network are independent of each other once containers are gone.
    if let Some(certs_volume) = resources.certs_volume.as_deref() {
        docker.remove_volume(certs_volume).await?;
    }
    docker.remove_network(&resources.network).await?;

    // Best-effort host-side socket dir cleanup. Same reason as
    // purge_container_filesystem above: the daemon socket and the
    // bind-mounted Capsule launch config live under
    // ~/.jackin/sockets/<container>/ and must be removed alongside the
    // docker-side teardown so re-launching the same container basename
    // does not inherit stale state.
    remove_socket_dir(paths, container_name).await;

    Ok(())
}
