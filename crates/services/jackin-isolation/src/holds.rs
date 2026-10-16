// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Durable preservation holds for workflow sources and checkpoints.
//!
//! Holds are stored in a versioned JSON file under the instance state
//! directory, so they survive host restarts. Normal cancellation, failure,
//! retention expiry, and prune must obey holds: [`check_removal`] is enforced
//! in the worktree/clone force-cleanup path and in foreground-session
//! finalization, not only in workflow CLI code.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;

const HOLDS_FILE: &str = "preservation-holds.json";
const HOLDS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreservationHold {
    pub resource_id: String,
    pub reason: String,
    pub operation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HoldsFile {
    version: u32,
    #[serde(default)]
    holds: Vec<PreservationHold>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldBlocksRemoval {
    pub resource_id: String,
    pub reason: String,
}

impl std::fmt::Display for HoldBlocksRemoval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "removal of `{}` is blocked by a preservation hold: {}",
            self.resource_id, self.reason
        )
    }
}

impl std::error::Error for HoldBlocksRemoval {}

fn holds_path(container_state_dir: &Path) -> std::path::PathBuf {
    container_state_dir.join(".jackin").join(HOLDS_FILE)
}

fn read_holds_file(container_state_dir: &Path) -> anyhow::Result<Vec<PreservationHold>> {
    let path = holds_path(container_state_dir);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(e)
                .with_context(|| format!("read preservation holds at {}", path.display()));
        }
    };
    let file: HoldsFile = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse preservation holds at {}", path.display()))?;
    anyhow::ensure!(
        file.version == HOLDS_VERSION,
        "unsupported preservation holds version {} at {}",
        file.version,
        path.display(),
    );
    Ok(file.holds)
}

fn write_holds_file(container_state_dir: &Path, holds: &[PreservationHold]) -> anyhow::Result<()> {
    let path = holds_path(container_state_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create holds dir at {}", parent.display()))?;
    }
    let body = serde_json::to_vec_pretty(&HoldsFile {
        version: HOLDS_VERSION,
        holds: holds.to_vec(),
    })?;
    std::fs::write(&path, &body)
        .with_context(|| format!("write preservation holds at {}", path.display()))?;
    Ok(())
}

/// Record a durable preservation hold on an owned source or checkpoint ID.
pub fn hold_work(
    container_state_dir: &Path,
    resource_id: &str,
    reason: &str,
    operation_id: Option<&str>,
) -> anyhow::Result<PreservationHold> {
    anyhow::ensure!(
        !resource_id.is_empty(),
        "preservation hold needs a resource ID"
    );
    anyhow::ensure!(!reason.is_empty(), "preservation hold needs a reason");
    let mut holds = read_holds_file(container_state_dir)?;
    if let Some(existing) = holds.iter().find(|h| h.resource_id == resource_id) {
        return Ok(existing.clone());
    }
    let hold = PreservationHold {
        resource_id: resource_id.to_owned(),
        reason: reason.to_owned(),
        operation_id: operation_id.map(str::to_owned),
    };
    holds.push(hold.clone());
    write_holds_file(container_state_dir, &holds)?;
    Ok(hold)
}

/// Returns `Ok` when removal is allowed, `Err(HoldBlocksRemoval)` when a
/// hold protects the resource.
pub fn check_removal(container_state_dir: &Path, resource_id: &str) -> anyhow::Result<()> {
    let holds = read_holds_file(container_state_dir)?;
    if let Some(hold) = holds.iter().find(|h| h.resource_id == resource_id) {
        return Err(HoldBlocksRemoval {
            resource_id: hold.resource_id.clone(),
            reason: hold.reason.clone(),
        }
        .into());
    }
    Ok(())
}

/// Release a preservation hold. Removing the last hold deletes nothing else.
pub fn release_hold(container_state_dir: &Path, resource_id: &str) -> anyhow::Result<bool> {
    let mut holds = read_holds_file(container_state_dir)?;
    let before = holds.len();
    holds.retain(|h| h.resource_id != resource_id);
    if holds.len() == before {
        return Ok(false);
    }
    write_holds_file(container_state_dir, &holds)?;
    Ok(true)
}

/// List all preservation holds for an instance state directory.
pub fn list_holds(container_state_dir: &Path) -> anyhow::Result<Vec<PreservationHold>> {
    read_holds_file(container_state_dir)
}

/// Workflow ownership labels applied to worker-owned containers.
///
/// The launch path copies these into the container labels so ownership can
/// be inspected host-side (`docker inspect`) without trusting worker state.
pub fn workflow_labels(run_id: &str, worker_id: &str) -> std::collections::HashMap<String, String> {
    [
        ("jackin.workflows/managed", "true"),
        ("jackin.workflows/run", run_id),
        ("jackin.workflows/worker", worker_id),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

#[cfg(test)]
mod tests;
