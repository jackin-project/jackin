//! Workspace isolation drift detection: find mounts whose `src` changed
//! while containers hold preserved isolation state.
//!
//! Previously lived in `config/workspaces/mod.rs`, which caused a
//! `config → runtime` edge. The function belongs here because it uses
//! `runtime::list_role_names` and `isolation::state`.

use anyhow::Context as _;

use crate::isolation::state::{IsolationRecord, list_records_for_workspace};

/// Outcome of a pre-edit drift check for a saved workspace.
///
/// `running_containers` are containers still running with preserved isolated
/// state for a mount whose `src` would be changed by the edit. The CLI
/// rejects the edit unconditionally — the operator must eject first.
///
/// `stopped_records` are the corresponding records on stopped containers.
/// The CLI requires `--delete-isolated-state` to drop them before applying
/// the edit.
#[derive(Debug, Clone, Default)]
pub struct DriftDetection {
    pub running_containers: Vec<String>,
    pub stopped_records: Vec<IsolationRecord>,
}

/// Classify isolation drift across every container that holds preserved
/// state for `workspace_name`.
///
/// A record drifts when its mount destination is no longer present in the
/// edited mounts, or when the new `src` differs from the `original_src`
/// recorded at materialization time. Drifted records on running containers
/// go into `running_containers`; the rest land in `stopped_records`.
pub async fn detect_workspace_edit_drift(
    paths: &crate::paths::JackinPaths,
    workspace_name: &str,
    edited_mounts: &[crate::workspace::MountConfig],
    docker: &impl crate::docker_client::DockerApi,
) -> anyhow::Result<DriftDetection> {
    let records = list_records_for_workspace(&paths.data_dir, workspace_name)?;
    if records.is_empty() {
        return Ok(DriftDetection::default());
    }
    let running = crate::runtime::list_role_names(docker, false)
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
