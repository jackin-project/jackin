// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Bulk instance prune and absent-for-purge ensures.

use crate::instance::{DockerResources, InstanceIndex};
use crate::runtime::prune_output;
use jackin_core::JackinPaths;

use jackin_core::CommandRunner;
use jackin_docker::docker_client::{ContainerState, DockerApi};

use super::{exile_all, purge_container_filesystem};

/// Force-eject all managed Docker resources then purge every instance's
/// state directory and index entry, regardless of status.
/// Used by `jackin prune instances --all` and `jackin prune system --all`.
pub async fn prune_all_instances(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    crate::runtime::coordination::ensure_prunable_async(paths, &paths.data_dir).await?;
    prune_output::section(
        "Instances",
        "stopping managed containers and removing all state",
    );
    exile_all(paths, docker).await?;

    jackin_host::caffeinate::reconcile(paths, docker, runner).await;

    let index = prune_output::start("Reading", "instance index")
        .complete(InstanceIndex::read_or_rebuild(&paths.data_dir), |error| {
            format!("could not read instance index: {error}")
        })?;
    if index.instances.is_empty() {
        prune_output::ok("no instances to prune");
    } else {
        let containers: Vec<String> = index
            .instances
            .iter()
            .map(|e| e.container_base.clone())
            .collect();

        let mut cleanup_failures = 0usize;
        for container_base in &containers {
            let row = prune_output::start("Deleting", container_base);
            if let Err(err) =
                purge_container_filesystem(paths, container_base, docker, runner).await
            {
                cleanup_failures += 1;
                row.failed(format!("isolation cleanup failed: {err}"));
            } else {
                row.ok();
            }
        }
        if cleanup_failures == 0 {
            prune_output::ok(format!("pruned {} instance(s)", containers.len()));
        } else {
            prune_output::failed(format!(
                "pruned {} instance(s), cleanup failed for {cleanup_failures}",
                containers.len()
            ));
        }
    }

    if let Err(err) = crate::isolation::safe_remove::safe_remove_dir_all(&paths.data_dir) {
        prune_output::failed("could not remove instance data");
        return Err(anyhow::Error::from(err).context(format!(
            "failed to remove instance data at {}",
            paths.data_dir.display()
        )));
    }
    Ok(())
}

pub(crate) async fn ensure_role_resources_absent_for_purge(
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
