// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Instance index and session vocabulary types.
//!
//! Pure data types describing a container instance's identity and status.
//! No IO, no Docker interaction. Shared between `jackin-runtime` (persistence)
//! and `jackin-console` (display) so neither crate depends on the other.

use serde::{Deserialize, Serialize};

use crate::agent::Agent;

/// Lifecycle status of a container instance in the host index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    /// Instance is the current active target for attach/restore.
    Active,
    /// Container process is running.
    Running,
    /// Exited cleanly with no preserved isolation.
    CleanExited,
    /// Container crashed or exited non-zero without preserved state.
    Crashed,
    /// Preserved because isolated worktrees still have dirty files.
    PreservedDirty,
    /// Preserved because isolated worktrees still have unpushed commits.
    PreservedUnpushed,
    /// Available to restore into a new launch.
    RestoreAvailable,
    /// Replaced by a newer instance for the same selector.
    Superseded,
    /// Index entry retained after purge for audit/history only.
    Purged,
    /// Setup failed before the instance became usable.
    FailedSetup,
}

impl InstanceStatus {
    /// Snake-case label matching the serde representation.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Running => "running",
            Self::CleanExited => "clean_exited",
            Self::Crashed => "crashed",
            Self::PreservedDirty => "preserved_dirty",
            Self::PreservedUnpushed => "preserved_unpushed",
            Self::RestoreAvailable => "restore_available",
            Self::Superseded => "superseded",
            Self::Purged => "purged",
            Self::FailedSetup => "failed_setup",
        }
    }

    /// Compact UI label for dense table rows where horizontal space is
    /// scarce. Lives on the type so adding a variant forces the renderer
    /// to update; a parallel free-function mapping silently drifts.
    #[must_use]
    pub const fn short_label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Running => "running",
            Self::CleanExited => "clean",
            Self::Crashed => "crashed",
            Self::PreservedDirty => "dirty",
            Self::PreservedUnpushed => "unpushed",
            Self::RestoreAvailable => "restore",
            Self::Superseded => "superseded",
            Self::Purged => "purged",
            Self::FailedSetup => "failed",
        }
    }
}

/// Status of one agent session inside an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Session process/tmux window is running.
    Running,
    /// Session has exited.
    Exited,
    /// Backing container no longer exists.
    ContainerMissing,
    /// Status could not be determined.
    Unknown,
}

/// Persisted record of one agent session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    /// Stable session identifier.
    pub session_id: String,
    /// Operator-facing session name.
    pub name: String,
    /// Agent runtime slug (`claude`, `codex`, …).
    pub agent_runtime: String,
    /// Tmux session/window name used for attach.
    pub tmux_name: String,
    /// Creation timestamp (ISO-8601 string).
    pub created_at: String,
    /// Current session status.
    pub status: SessionStatus,
    /// Last attach timestamp when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_attached_at: Option<String>,
}

/// One row in the host instance index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceIndexEntry {
    /// Stable instance identifier.
    pub instance_id: String,
    /// Docker container base name (without transient suffixes).
    pub container_base: String,
    /// Config workspace name when launched from a named workspace.
    pub workspace_name: Option<String>,
    /// Display label for the workspace/directory.
    pub workspace_label: String,
    /// Host workdir used at launch.
    pub workdir: String,
    /// Role key this instance was built for.
    pub role_key: String,
    /// Agent runtime slug.
    pub agent_runtime: String,
    /// Lifecycle status.
    pub status: InstanceStatus,
    /// Last update timestamp (ISO-8601 string).
    pub updated_at: String,
}

impl InstanceIndexEntry {
    /// Whether this entry matches the selector dimensions in `query`.
    pub fn matches(&self, query: InstanceQuery<'_>) -> bool {
        self.workspace_name.as_deref() == query.workspace_name
            && self.workspace_label == query.workspace_label
            && self.workdir == query.workdir
            && query
                .role_key
                .is_none_or(|role_key| self.role_key == role_key)
            && query
                .agent_runtime
                .is_none_or(|agent| self.agent_runtime == agent.slug())
    }
}

/// Query dimensions for looking up an [`InstanceIndexEntry`].
#[derive(Debug, Clone, Copy)]
pub struct InstanceQuery<'a> {
    /// Optional config workspace name.
    pub workspace_name: Option<&'a str>,
    /// Workspace/directory display label.
    pub workspace_label: &'a str,
    /// Host workdir path.
    pub workdir: &'a str,
    /// Optional role key filter.
    pub role_key: Option<&'a str>,
    /// Optional agent filter.
    pub agent_runtime: Option<Agent>,
}
