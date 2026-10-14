// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::isolation::MountIsolation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupStatus {
    Active,

    PreservedDirty,

    PreservedUnpushed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IsolationRecord {
    #[serde(deserialize_with = "deserialize_workspace_identity")]
    pub workspace_name: Option<crate::WorkspaceName>,

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

#[derive(Debug, Clone, Default)]
pub struct DriftDetection {
    pub running_containers: Vec<String>,

    pub stopped_records: Vec<IsolationRecord>,
}

fn deserialize_workspace_identity<'de, D>(
    deserializer: D,
) -> Result<Option<crate::WorkspaceName>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<crate::WorkspaceName>::deserialize(deserializer)
}
