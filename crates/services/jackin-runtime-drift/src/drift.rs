// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;

use jackin_runtime_isolation::isolation::state::list_records_for_workspace;

pub use jackin_core::DriftDetection;

pub async fn detect_workspace_edit_drift(
    paths: &jackin_core::JackinPaths,
    workspace_name: &jackin_core::WorkspaceName,
    edited_mounts: &[jackin_config::MountConfig],
    docker: &impl jackin_docker::docker_client::DockerApi,
) -> anyhow::Result<DriftDetection> {
    let records = list_records_for_workspace(&paths.data_dir, workspace_name)?;
    if records.is_empty() {
        return Ok(DriftDetection::default());
    }
    let running = jackin_runtime_discovery::discovery::list_role_names(docker, false)
        .await
        .context("listing running containers to check for workspace edit drift")?;

    let mut affected_running = Vec::new();
    let mut affected_stopped = Vec::new();
    for rec in records {
        let edited = edited_mounts.iter().find(|m| m.dst == rec.mount_dst);
        let drifted = edited.is_none_or(|m| m.src != rec.original_src);
        if !drifted {
            continue;
        }
        if running.iter().any(|n| n == &rec.container_name) {
            affected_running.push(rec.container_name.clone());
        } else {
            affected_stopped.push(rec);
        }
    }
    Ok(DriftDetection {
        running_containers: affected_running,
        stopped_records: affected_stopped,
    })
}
#[cfg(test)]
mod tests;
