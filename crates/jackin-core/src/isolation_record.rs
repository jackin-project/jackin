// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Isolation record data types: the persistent state written for each mount
//! under an isolated workspace.
//!
//! Pure data only — no filesystem IO. IO lives in `jackin-runtime`.

use serde::{Deserialize, Serialize};

use crate::isolation::MountIsolation;

/// Current cleanup state recorded in `isolation.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupStatus {
    Active,
    PreservedDirty,
    PreservedUnpushed,
}

/// One isolated mount entry persisted inside the container state directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsolationRecord {
    pub workspace: String,
    pub mount_dst: String,
    pub original_src: String,
    pub isolation: MountIsolation,
    pub worktree_path: String,
    pub scratch_branch: String,
    pub base_commit: String,
    pub selector_key: String,
    pub container_name: String,
    pub cleanup_status: CleanupStatus,
}

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
