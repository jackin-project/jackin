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
    /// Isolation is live and eligible for normal teardown.
    Active,
    /// Kept after exit because the worktree still has uncommitted changes.
    PreservedDirty,
    /// Kept after exit because local commits are not yet pushed.
    PreservedUnpushed,
}

/// One isolated mount entry persisted inside the container state directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsolationRecord {
    /// Workspace name this mount belongs to.
    pub workspace: String,
    /// Container-side mount destination path.
    pub mount_dst: String,
    /// Host path that was isolated (original workspace `src`).
    pub original_src: String,
    /// Isolation strategy used for this mount.
    pub isolation: MountIsolation,
    /// Host path of the git worktree (empty for non-worktree isolation).
    pub worktree_path: String,
    /// Scratch branch name created for worktree isolation.
    pub scratch_branch: String,
    /// Base commit SHA the isolation was created from.
    pub base_commit: String,
    /// Selector key that identified the session/container.
    pub selector_key: String,
    /// Docker container name that owns this record.
    pub container_name: String,
    /// Current cleanup/preservation status.
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
    /// Names of still-running containers with preserved isolated state for a changed mount.
    pub running_containers: Vec<String>,
    /// Isolation records on stopped containers that would be invalidated by the edit.
    pub stopped_records: Vec<IsolationRecord>,
}
