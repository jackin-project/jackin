// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Cleanup handle resolution for recorded state.

use jackin_core::JackinPaths;
use jackin_instance::{DockerResources, InstanceManifest};

use jackin_core::ContainerHandle;
use jackin_docker::docker_client::{ContainerState, DockerApi};

/// One immutable authority snapshot for the complete destructive preflight.
pub async fn resolve_cleanup_handles_for_state(
    paths: &JackinPaths,
    container_name: &str,
    known_role: Option<&ContainerHandle>,
    docker: &impl DockerApi,
) -> anyhow::Result<(
    DockerResources,
    Option<ContainerHandle>,
    Option<ContainerHandle>,
)> {
    let manifest = InstanceManifest::read_optional(&paths.data_dir.join(container_name))?;
    let resources = manifest.as_ref().map_or_else(
        || DockerResources::from_container_name(container_name),
        |manifest| manifest.docker.clone(),
    );
    let identity = manifest
        .as_ref()
        .and_then(|manifest| manifest.docker_identity.as_ref());
    if let Some(known) = known_role {
        let identity = identity.ok_or_else(|| {
            anyhow::anyhow!("Docker ownership identity unavailable for {container_name}")
        })?;
        anyhow::ensure!(
            known.name() == container_name && known.id() == identity.role_container_id,
            "role container ownership identity mismatch before destructive cleanup for {container_name}"
        );
    }
    let role = resolve_owned_container_handle(
        docker,
        container_name,
        identity.map(|identity| identity.role_container_id.as_str()),
    )
    .await?;
    let dind = match resources.dind_container.as_deref() {
        Some(name) => {
            resolve_owned_container_handle(
                docker,
                name,
                identity.and_then(|identity| identity.dind_container_id.as_deref()),
            )
            .await?
        }
        None => None,
    };
    Ok((resources, role, dind))
}

/// Resolve only identities belonging to the recorded launch. A name lookup
/// checks absence/replacement; it can never establish ownership.
pub async fn resolve_role_handle_for_state(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<ContainerHandle> {
    resolve_optional_role_handle_for_state(paths, container_name, docker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("container {container_name} is not present"))
}

pub(crate) async fn resolve_optional_role_handle_for_state(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<ContainerHandle>> {
    let manifest = InstanceManifest::read_optional(&paths.data_dir.join(container_name))?;
    let expected_id = manifest
        .as_ref()
        .and_then(|manifest| manifest.docker_identity.as_ref())
        .map(|identity| identity.role_container_id.as_str());
    resolve_owned_container_handle(docker, container_name, expected_id).await
}

pub async fn resolve_dind_handle_for_state(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<Option<ContainerHandle>> {
    let manifest = InstanceManifest::read_optional(&paths.data_dir.join(container_name))?;
    let resources = manifest.as_ref().map_or_else(
        || DockerResources::from_container_name(container_name),
        |manifest| manifest.docker.clone(),
    );
    let expected_id = manifest
        .as_ref()
        .and_then(|manifest| manifest.docker_identity.as_ref())
        .and_then(|identity| identity.dind_container_id.as_deref());
    match resources.dind_container.as_deref() {
        Some(name) => resolve_owned_container_handle(docker, name, expected_id).await,
        None => Ok(None),
    }
}

pub(crate) async fn resolve_owned_container_handle(
    docker: &impl DockerApi,
    name: &str,
    expected_id: Option<&str>,
) -> anyhow::Result<Option<ContainerHandle>> {
    let Some(handle) = resolve_optional_container_handle(docker, name).await? else {
        return Ok(None);
    };
    let expected_id = expected_id.filter(|id| !id.is_empty()).ok_or_else(|| anyhow::anyhow!(
        "Docker ownership identity unavailable for {name}; refusing lifecycle changes; recover the original launch identity explicitly"
    ))?;
    anyhow::ensure!(
        handle.id() == expected_id,
        "Docker ownership identity mismatch for {name}: recorded {expected_id}, found {}; refusing lifecycle changes",
        handle.id()
    );
    Ok(Some(ContainerHandle::new(name, expected_id)?))
}

pub async fn resolve_optional_container_handle(
    docker: &impl DockerApi,
    name: &str,
) -> anyhow::Result<Option<ContainerHandle>> {
    let inspection = docker.inspect_container_by_name(name).await;
    match inspection.handle {
        Some(handle) => Ok(Some(handle)),
        None if matches!(inspection.state, ContainerState::NotFound) => Ok(None),
        None => anyhow::bail!(
            "cannot resolve container {name}: {}",
            inspection.state.inspect_label()
        ),
    }
}

pub fn docker_resources_for_state(
    paths: &JackinPaths,
    container_name: &str,
) -> anyhow::Result<DockerResources> {
    let manifest = InstanceManifest::read_optional(&paths.data_dir.join(container_name))?;
    Ok(manifest.map_or_else(
        || DockerResources::from_container_name(container_name),
        |manifest| manifest.docker,
    ))
}
