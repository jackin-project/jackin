// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Mount-info refresh plans.

#[derive(Debug)]
pub struct PendingMountInfoRefresh {
    pub target: MountInfoRefreshTarget,
    pub entries: Vec<(String, crate::mount_info::MountKind)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountInfoRefreshTarget {
    ManagerList,
    Editor,
    SettingsMounts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountInfoRefreshSourcePlan {
    pub target: MountInfoRefreshTarget,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MountInfoRefreshSourceFacts {
    ManagerList {
        current_dir: String,
        workspace_mount_sources: Vec<String>,
        global_mount_sources: Vec<String>,
    },
    Editor {
        mount_sources: Vec<String>,
    },
    SettingsMounts {
        mount_sources: Vec<String>,
    },
    Inactive,
}

#[must_use]
pub fn mount_info_refresh_source_plan(
    facts: MountInfoRefreshSourceFacts,
) -> Option<MountInfoRefreshSourcePlan> {
    let (target, sources) = match facts {
        MountInfoRefreshSourceFacts::ManagerList {
            current_dir,
            workspace_mount_sources,
            global_mount_sources,
        } => {
            let sources = std::iter::once(current_dir)
                .chain(workspace_mount_sources)
                .chain(global_mount_sources);
            (MountInfoRefreshTarget::ManagerList, dedup_sources(sources))
        }
        MountInfoRefreshSourceFacts::Editor { mount_sources } => {
            (MountInfoRefreshTarget::Editor, dedup_sources(mount_sources))
        }
        MountInfoRefreshSourceFacts::SettingsMounts { mount_sources } => (
            MountInfoRefreshTarget::SettingsMounts,
            dedup_sources(mount_sources),
        ),
        MountInfoRefreshSourceFacts::Inactive => return None,
    };

    (!sources.is_empty()).then_some(MountInfoRefreshSourcePlan { target, sources })
}

pub(crate) fn dedup_sources(sources: impl IntoIterator<Item = String>) -> Vec<String> {
    sources
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}
