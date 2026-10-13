// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ManagerState` mount-refresh application.

use crate::tui::message::{MountInfoRefreshSourceFacts, mount_info_refresh_source_plan};
use jackin_config::AppConfig;

use super::super::{ManagerStage, ManagerState, MountInfoRefreshTarget, PendingMountInfoRefresh};

impl ManagerState<'_> {
    pub fn apply_mount_info_refresh(&mut self, result: PendingMountInfoRefresh) -> bool {
        match result.target {
            MountInfoRefreshTarget::ManagerList => {
                self.mount_info_cache.store_entries(result.entries);
            }
            MountInfoRefreshTarget::Editor => {
                let ManagerStage::Editor(editor) = &mut self.stage else {
                    return false;
                };
                editor.mount_info_cache.store_entries(result.entries);
            }
            MountInfoRefreshTarget::SettingsMounts => {
                let ManagerStage::Settings(settings) = &mut self.stage else {
                    return false;
                };
                settings
                    .mounts
                    .mount_info_cache
                    .store_entries(result.entries);
            }
        }
        true
    }

    pub fn active_mount_info_sources(
        &self,
        config: &AppConfig,
    ) -> Option<(MountInfoRefreshTarget, Vec<String>)> {
        let facts = match &self.stage {
            ManagerStage::List => MountInfoRefreshSourceFacts::ManagerList {
                current_dir: self.current_dir.clone(),
                workspace_mount_sources: config
                    .workspaces
                    .values()
                    .flat_map(|workspace| workspace.mounts.iter().map(|mount| mount.src.clone()))
                    .collect(),
                global_mount_sources: config
                    .list_mount_rows()
                    .into_iter()
                    .map(|row| row.mount.src)
                    .collect(),
            },
            ManagerStage::Editor(editor) => MountInfoRefreshSourceFacts::Editor {
                mount_sources: editor
                    .pending
                    .mounts
                    .iter()
                    .map(|mount| mount.src.clone())
                    .collect(),
            },
            ManagerStage::Settings(settings) => MountInfoRefreshSourceFacts::SettingsMounts {
                mount_sources: settings
                    .mounts
                    .pending
                    .iter()
                    .map(|row| row.mount.src.clone())
                    .collect(),
            },
            ManagerStage::CreatePrelude(_)
            | ManagerStage::ConfirmDelete { .. }
            | ManagerStage::ConfirmInstancePurge { .. } => MountInfoRefreshSourceFacts::Inactive,
        };

        mount_info_refresh_source_plan(facts).map(|plan| (plan.target, plan.sources))
    }
}
