// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Instance index and session vocabulary types.
//!
//! Pure data types describing a container instance's identity and status.
//! No IO, no Docker interaction. Shared between `jackin-runtime` (persistence)
//! and `jackin-console` (display) so neither crate depends on the other.

use serde::{Deserialize, Serialize};

use crate::agent::Agent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Active,
    Running,
    CleanExited,
    Crashed,
    PreservedDirty,
    PreservedUnpushed,
    RestoreAvailable,
    Superseded,
    Purged,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Exited,
    ContainerMissing,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub name: String,
    pub agent_runtime: String,
    pub tmux_name: String,
    pub created_at: String,
    pub status: SessionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_attached_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceIndexEntry {
    pub instance_id: String,
    pub container_base: String,
    pub workspace_name: Option<String>,
    pub workspace_label: String,
    pub workdir: String,
    pub role_key: String,
    pub agent_runtime: String,
    pub status: InstanceStatus,
    pub updated_at: String,
}

impl InstanceIndexEntry {
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

#[derive(Debug, Clone, Copy)]
pub struct InstanceQuery<'a> {
    pub workspace_name: Option<&'a str>,
    pub workspace_label: &'a str,
    pub workdir: &'a str,
    pub role_key: Option<&'a str>,
    pub agent_runtime: Option<Agent>,
}
