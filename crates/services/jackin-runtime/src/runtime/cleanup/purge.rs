// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Class data and container state purge.

use crate::instance::InstanceIndex;
use jackin_core::CommandRunner;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_docker::docker_client::DockerApi;

use super::{cleanup_failure, cleanup_timing, ensure_backend_absent_for_purge, remove_socket_dir};

pub async fn purge_class_data(
    paths: &JackinPaths,
    selector: &RoleSelector,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let _timing = cleanup_timing("class_data");
    if !paths.data_dir.exists() {
        return Ok(());
    }

    // Drive each filesystem teardown to completion, then batch the
    // index update for whichever containers succeeded. Returning early
    // on the first failure without recording the prior successes would
    // leave the index claiming the already-deleted state dirs still
    // hold their pre-purge status.
    let role_slug = crate::instance::naming::compact_component(&selector.name, "role");
    let mut matched = Vec::new();
    let mut first_error: Option<anyhow::Error> = None;
    for entry in std::fs::read_dir(&paths.data_dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !crate::instance::naming::class_family_matches_with_slug(&role_slug, &file_name) {
            continue;
        }
        match purge_container_filesystem(paths, &file_name, docker, runner).await {
            Ok(()) => matched.push(file_name),
            Err(error) => {
                cleanup_failure(format!("class data purge failed: {error}"));
                first_error = Some(error);
                break;
            }
        }
    }
    let refs: Vec<&str> = matched.iter().map(String::as_str).collect();
    let mark_err = InstanceIndex::mark_many_purged(&paths.data_dir, &refs);
    if let Some(err) = first_error {
        return Err(err);
    }
    mark_err
}

pub async fn purge_container_state(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let _timing = cleanup_timing("container_state");
    purge_container_filesystem(paths, container_name, docker, runner).await?;
    InstanceIndex::mark_purged(&paths.data_dir, container_name)
}

/// Per-container filesystem teardown (docker-state guard + isolation
/// cleanup + state directory removal). Index updates are batched by the
/// caller so multi-container purges avoid an O(M²) read-rewrite cycle.
pub(crate) async fn purge_container_filesystem(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let _timing = cleanup_timing("container_filesystem");
    crate::runtime::coordination::ensure_prunable_async(
        paths,
        &paths.data_dir.join(container_name),
    )
    .await?;
    ensure_backend_absent_for_purge(paths, container_name, docker).await?;
    crate::isolation::cleanup::purge_isolated_for_container(
        &paths.data_dir.join(container_name),
        runner,
    )
    .await?;
    let state_dir = paths.data_dir.join(container_name);
    // Owned-validated-path removal: the container name is operator/index
    // input, so deletion is containment-bound to the data dir and fd-pinned
    // (`O_NOFOLLOW` at every level). Escapes and symlinks are refused
    // loudly instead of followed; a missing dir is still a no-op.
    crate::isolation::safe_remove::safe_remove_dir_contained(&paths.data_dir, &state_dir)?;
    // Remove the host-side bind-mount dir (~/.jackin/sockets/<container>/)
    // that holds the daemon socket and Capsule launch config. Skipping it
    // here leaks stale `agent.toml` across load/purge cycles; a future
    // launch with the same container basename would bind-mount the old
    // contents before the host's mkdir + write overwrites them.
    remove_socket_dir(paths, container_name).await;
    // Coordination inodes live outside the purged runtime state and persist.
    Ok(())
}
