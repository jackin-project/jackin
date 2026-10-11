// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Absent-for-purge guards.
//!
//! [`ensure_role_resources_absent_for_purge`] refuses local-state purge
//! while the role container (or its `DinD` sidecar) still exists in Docker,
//! directing the operator to `jackin eject --purge` instead. A container
//! whose state cannot even be inspected also refuses: purging blind
//! would orphan Docker resources. Bulk prune and filesystem teardown
//! stay in the `jackin-runtime` hub.

use jackin_docker::docker_client::{ContainerState, DockerApi};
use jackin_instance::DockerResources;

pub async fn ensure_role_resources_absent_for_purge(
    docker: &impl DockerApi,
    resources: &DockerResources,
) -> anyhow::Result<()> {
    ensure_container_absent_for_purge(docker, &resources.role_container, "role container").await?;
    if let Some(dind_container) = resources.dind_container.as_deref() {
        ensure_container_absent_for_purge(docker, dind_container, "DinD sidecar").await?;
    }
    Ok(())
}

pub(crate) async fn ensure_container_absent_for_purge(
    docker: &impl DockerApi,
    container_name: &str,
    resource_label: &str,
) -> anyhow::Result<()> {
    let state_phrase = match docker.inspect_container_by_name(container_name).await.state {
        ContainerState::NotFound => return Ok(()),
        ContainerState::Running => "and is running",
        ContainerState::Paused => "and is paused",
        ContainerState::Restarting => "and is restarting",
        ContainerState::Created => "and is being created",
        ContainerState::Removing => "and is being removed",
        ContainerState::Dead => "but is dead",
        ContainerState::Stopped { .. } => "but is stopped",
        ContainerState::InspectUnavailable(reason) => {
            anyhow::bail!(
                "cannot purge local state for `{container_name}` because Docker resource state could not be inspected: {reason}"
            )
        }
    };
    anyhow::bail!(
        "cannot purge local state because {resource_label} `{container_name}` still exists {state_phrase}; run `jackin eject {container_name} --purge` to remove Docker resources and local state together"
    )
}
