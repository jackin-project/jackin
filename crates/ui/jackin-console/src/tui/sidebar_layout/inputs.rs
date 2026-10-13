// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Sidebar input facts and instance queries.

use super::{SidebarScrollFocus, agents_block_agent_count_for_config};

use crate::mount_info_cache::MountInfoCache;

/// Shared facts for the right-pane sidebar body. Root adapters supply concrete
/// workspace/config rows; crate-owned layout helpers consume the generic shape.
#[derive(Debug)]
pub struct SidebarInputs<'a, Mount, WorkspaceConfig, GlobalMountRow, MountInfoCache> {
    pub workdir: &'a str,
    pub mounts: &'a [Mount],
    pub mount_info_cache: MountInfoCache,
    pub ws_config: Option<&'a WorkspaceConfig>,
    pub global_rows: Vec<GlobalMountRow>,
    pub picker_role_label: String,
    pub instance_count: usize,
    pub instance_expanded: bool,
    pub inline_picker_active: bool,
    pub show_envs: bool,
    pub agent_count: usize,
}

pub type ConfigSidebarInputs<'a> = SidebarInputs<
    'a,
    jackin_config::MountConfig,
    jackin_config::WorkspaceConfig,
    jackin_config::GlobalMountRow,
    MountInfoCache,
>;

/// Facts needed to build the config-backed workspace preview sidebar. Root
/// supplies only root-specific counts and selected rows; the layout crate owns
/// the reusable sidebar input assembly.
#[derive(Debug)]
pub struct ConfigSidebarSelectionInputs<'a> {
    pub workdir: &'a str,
    pub mounts: &'a [jackin_config::MountConfig],
    pub mount_info_cache: MountInfoCache,
    pub ws_config: Option<&'a jackin_config::WorkspaceConfig>,
    pub global_rows: Vec<jackin_config::GlobalMountRow>,
    pub picker_role_label: String,
    pub instance_count: usize,
    pub instance_expanded: bool,
    pub inline_picker_active: bool,
    pub show_envs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarInstanceFacts<'a> {
    pub workspace_name: Option<&'a str>,
    pub workspace_label: &'a str,
    pub workdir: &'a str,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarInstanceQuery<'a> {
    pub workspace_name: Option<&'a str>,
    pub workspace_label: &'a str,
    pub workdir: &'a str,
}

#[must_use]
pub fn sidebar_active_instance_count<'a>(
    instances: impl IntoIterator<Item = SidebarInstanceFacts<'a>>,
    query: SidebarInstanceQuery<'a>,
) -> usize {
    instances
        .into_iter()
        .filter(|instance| {
            instance.active
                && instance.workspace_name == query.workspace_name
                && instance.workspace_label == query.workspace_label
                && instance.workdir == query.workdir
        })
        .count()
}

#[must_use]
pub fn config_sidebar_inputs_for_selection<'a>(
    selection: ConfigSidebarSelectionInputs<'a>,
    config: &'a jackin_config::AppConfig,
) -> ConfigSidebarInputs<'a> {
    let agent_count = if selection.inline_picker_active {
        0
    } else {
        agents_block_agent_count_for_config(selection.ws_config, config)
    };

    ConfigSidebarInputs {
        workdir: selection.workdir,
        mounts: selection.mounts,
        mount_info_cache: selection.mount_info_cache,
        ws_config: selection.ws_config,
        global_rows: selection.global_rows,
        picker_role_label: selection.picker_role_label,
        instance_count: selection.instance_count,
        instance_expanded: selection.instance_expanded,
        inline_picker_active: selection.inline_picker_active,
        show_envs: selection.show_envs,
        agent_count,
    }
}

impl From<crate::tui::focus::MountScrollFocus> for SidebarScrollFocus {
    fn from(focus: crate::tui::focus::MountScrollFocus) -> Self {
        match focus {
            crate::tui::focus::MountScrollFocus::Workspace => Self::Workspace,
            crate::tui::focus::MountScrollFocus::Global => Self::Global,
            crate::tui::focus::MountScrollFocus::RoleGlobal => Self::RoleGlobal,
            crate::tui::focus::MountScrollFocus::Roles => Self::Roles,
        }
    }
}
