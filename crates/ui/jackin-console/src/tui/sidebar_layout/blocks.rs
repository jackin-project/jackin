// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Sidebar block height planners.

use super::mount_data_row_count;

use crate::mount_info_cache::MountInfoCache;
use crate::tui::mount_display::global_config_mounts_content_width_with_cache;

#[must_use]
pub fn agents_block_height(agent_count: usize) -> u16 {
    let agent_rows = agent_count.max(1);
    (2 + 1 + 1 + agent_rows).min(14) as u16
}

#[must_use]
pub fn mount_block_height(same_path_rows: impl IntoIterator<Item = bool>) -> u16 {
    let data_rows = mount_data_row_count(same_path_rows).unwrap_or(1);
    (data_rows + 2 + 1).min(12) as u16
}

#[must_use]
pub fn global_mount_rows_height(same_path_rows: impl IntoIterator<Item = bool>) -> u16 {
    let content_height = global_mounts_content_height(same_path_rows);
    (content_height + 2).min(12) as u16
}

#[must_use]
pub fn global_mounts_content_height(same_path_rows: impl IntoIterator<Item = bool>) -> usize {
    mount_data_row_count(same_path_rows).map_or(1, |data_rows| 1 + data_rows)
}

#[must_use]
pub fn env_block_height(workspace_keys: usize, role_keys: usize) -> u16 {
    let total_rows = workspace_keys + role_keys;
    (total_rows + 2).min(20) as u16
}

#[must_use]
pub fn env_block_height_for_config(ws_config: Option<&jackin_config::WorkspaceConfig>) -> u16 {
    let Some(ws) = ws_config else {
        return 2;
    };

    let workspace_keys = ws.env.len();
    let role_keys: usize = ws.roles.values().map(|o| o.env.len()).sum();
    env_block_height(workspace_keys, role_keys)
}

#[must_use]
pub const fn workspace_has_any_env(workspace_keys: usize, role_keys: usize) -> bool {
    workspace_keys > 0 || role_keys > 0
}

#[must_use]
pub const fn agents_block_agent_count(
    all_allowed: bool,
    role_count: usize,
    allowed_role_count: usize,
) -> usize {
    if all_allowed {
        role_count
    } else {
        allowed_role_count
    }
}

#[must_use]
pub fn agents_block_agent_count_for_config(
    ws_config: Option<&jackin_config::WorkspaceConfig>,
    config: &jackin_config::AppConfig,
) -> usize {
    let all_allowed = ws_config.is_none_or(crate::workspace::allows_all_agents);
    let allowed_role_count = ws_config.map_or(0, |w| w.allowed_roles.len());
    agents_block_agent_count(all_allowed, config.roles.len(), allowed_role_count)
}

#[must_use]
pub fn agents_block_content_width<S>(role_keys: impl IntoIterator<Item = S>) -> usize
where
    S: AsRef<str>,
{
    role_keys
        .into_iter()
        .map(|key| termrock::text::display_cols(key.as_ref()) + 4)
        .max()
        .unwrap_or(0)
}

pub(crate) fn config_global_mount_rows_height(rows: &[&jackin_config::GlobalMountRow]) -> u16 {
    global_mount_rows_height(rows.iter().map(|row| row.mount.src == row.mount.dst))
}

pub(crate) fn global_mounts_content_width_from_rows(
    rows: &[&jackin_config::GlobalMountRow],
    cache: &MountInfoCache,
) -> usize {
    let mounts: Vec<jackin_config::MountConfig> =
        rows.iter().map(|row| row.mount.clone()).collect();
    global_config_mounts_content_width_with_cache(&mounts, cache)
}

pub(crate) fn global_mounts_content_height_from_rows(
    rows: &[&jackin_config::GlobalMountRow],
) -> usize {
    global_mounts_content_height(rows.iter().map(|row| row.mount.src == row.mount.dst))
}
