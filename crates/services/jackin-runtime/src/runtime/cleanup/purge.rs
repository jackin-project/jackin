// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

    crate::isolation::safe_remove::safe_remove_dir_contained(&paths.data_dir, &state_dir)?;

    remove_socket_dir(paths, container_name).await;

    Ok(())
}
