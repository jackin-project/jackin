// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Pure sidebar rectangle allocation for the workspace list preview pane.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use termrock::layout::ScrollAxes;

use crate::mount_info_cache::MountInfoCache;
use crate::tui::mount_display::{
    global_config_mounts_content_width_with_cache, workspace_config_mounts_content_height,
    workspace_config_mounts_content_width_with_cache,
};
use crate::tui::screens::workspaces::model::ManagerListRow;

/// Fixed height of the compact running-instances badge (borders + 1 text line).
pub const COMPACT_INSTANCES_HEIGHT: u16 = 3;

/// Root-derived heights and visibility flags for sidebar layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarLayoutMetrics {
    pub instance_count: usize,
    pub workspace_mount_height: u16,
    pub global_mount_height: Option<u16>,
    pub role_global_mount_height: Option<u16>,
    pub env_height: Option<u16>,
    pub show_roles: bool,
    pub agent_count: usize,
}

/// Rect for each rendered block. `None` panels are skipped in both render
/// and hit-test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarLayout {
    pub instances: Option<Rect>,
    pub general: Rect,
    pub mounts: Rect,
    pub global: Option<Rect>,
    pub role_global: Option<Rect>,
    pub env: Option<Rect>,
    pub roles: Option<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarScrollArea {
    pub area: Rect,
    pub content_width: usize,
    pub content_height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarScrollAreas {
    pub workspace: SidebarScrollArea,
    pub global: SidebarScrollArea,
    pub role_global: Option<SidebarScrollArea>,
    pub roles: Option<SidebarScrollArea>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarScrollFocus {
    Workspace,
    Global,
    RoleGlobal,
    Roles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectedSidebarTarget {
    CurrentDirectory,
    SavedWorkspace(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountRowsSelection<Role> {
    None,
    CurrentDirectory,
    SavedWorkspace { picker_role: Option<Role> },
}

#[must_use]
pub const fn selected_sidebar_target(row: ManagerListRow) -> Option<SelectedSidebarTarget> {
    match row {
        ManagerListRow::CurrentDirectory => Some(SelectedSidebarTarget::CurrentDirectory),
        ManagerListRow::SavedWorkspace(idx) => Some(SelectedSidebarTarget::SavedWorkspace(idx)),
        ManagerListRow::NewWorkspace
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::CurrentDirectoryInstance(_) => None,
    }
}

#[must_use]
pub fn global_mount_rows_selection<Role>(
    row: ManagerListRow,
    saved_workspace_exists: impl FnOnce(usize) -> bool,
    picker_role: Option<Role>,
) -> GlobalMountRowsSelection<Role> {
    match row {
        ManagerListRow::CurrentDirectory | ManagerListRow::CurrentDirectoryInstance(_) => {
            GlobalMountRowsSelection::CurrentDirectory
        }
        ManagerListRow::SavedWorkspace(idx) if saved_workspace_exists(idx) => {
            GlobalMountRowsSelection::SavedWorkspace { picker_role }
        }
        ManagerListRow::SavedWorkspace(_)
        | ManagerListRow::NewWorkspace
        | ManagerListRow::WorkspaceInstance(_, _) => GlobalMountRowsSelection::None,
    }
}

#[must_use]
pub fn inline_picker_role<Role>(
    selected_role_picker_role: Option<Role>,
    agent_picker_role: Option<Role>,
) -> Option<Role> {
    selected_role_picker_role.or(agent_picker_role)
}

#[must_use]
pub const fn inline_picker_active(role_picker_open: bool, agent_picker_open: bool) -> bool {
    role_picker_open || agent_picker_open
}

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

#[must_use]
pub fn focused_mount_scroll_area_still_scrollable(
    focus: crate::tui::focus::MountScrollFocus,
    areas: Option<&SidebarScrollAreas>,
) -> bool {
    focused_scroll_area_still_scrollable(focus.into(), areas)
}

#[must_use]
pub fn focused_scroll_area_still_scrollable(
    focus: SidebarScrollFocus,
    areas: Option<&SidebarScrollAreas>,
) -> bool {
    focused_scroll_area_axes(focus, areas).any()
}

#[must_use]
pub fn focused_scroll_area_axes(
    focus: SidebarScrollFocus,
    areas: Option<&SidebarScrollAreas>,
) -> ScrollAxes {
    let Some(areas) = areas else {
        return ScrollAxes::none();
    };
    match focus {
        SidebarScrollFocus::Workspace => scroll_area_axes(areas.workspace),
        SidebarScrollFocus::Global => {
            if areas.global.area.height > 0 {
                scroll_area_axes(areas.global)
            } else {
                ScrollAxes::none()
            }
        }
        SidebarScrollFocus::RoleGlobal => areas
            .role_global
            .map_or_else(ScrollAxes::none, scroll_area_axes),
        SidebarScrollFocus::Roles => areas.roles.map_or_else(ScrollAxes::none, scroll_area_axes),
    }
}

#[must_use]
pub fn compute_sidebar_layout(area: Rect, metrics: SidebarLayoutMetrics) -> SidebarLayout {
    let mut constraints = Vec::new();
    if metrics.instance_count > 0 {
        constraints.push(Constraint::Length(COMPACT_INSTANCES_HEIGHT));
    }
    constraints.push(Constraint::Length(3));
    constraints.push(Constraint::Length(metrics.workspace_mount_height));
    if let Some(height) = metrics.global_mount_height {
        constraints.push(Constraint::Length(height));
    }
    if let Some(height) = metrics.role_global_mount_height {
        constraints.push(Constraint::Length(height));
    }
    if let Some(height) = metrics.env_height {
        constraints.push(Constraint::Length(height));
    }
    if metrics.show_roles {
        constraints.push(Constraint::Length(agents_block_height(metrics.agent_count)));
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    let mut iter = rows.iter().copied();
    let mut next_row = || iter.next().unwrap_or(Rect::default());

    SidebarLayout {
        instances: (metrics.instance_count > 0).then(&mut next_row),
        general: next_row(),
        mounts: next_row(),
        global: metrics.global_mount_height.is_some().then(&mut next_row),
        role_global: metrics
            .role_global_mount_height
            .is_some()
            .then(&mut next_row),
        env: metrics.env_height.is_some().then(&mut next_row),
        roles: metrics.show_roles.then(&mut next_row),
    }
}

#[must_use]
pub fn compute_config_sidebar_layout(
    area: Rect,
    inputs: &ConfigSidebarInputs<'_>,
) -> SidebarLayout {
    let (global_rows, role_global_rows) =
        crate::services::workspace::split_global_mount_rows(&inputs.global_rows);
    let show_global_header = !global_rows.is_empty() || role_global_rows.is_empty();
    let show_global = !inputs.global_rows.is_empty() && show_global_header;
    let show_role_global = !role_global_rows.is_empty();
    let show_roles = !inputs.inline_picker_active;

    compute_sidebar_layout(
        area,
        SidebarLayoutMetrics {
            instance_count: inputs.instance_count,
            workspace_mount_height: mount_block_height(
                inputs.mounts.iter().map(|mount| mount.src == mount.dst),
            ),
            global_mount_height: show_global.then(|| config_global_mount_rows_height(&global_rows)),
            role_global_mount_height: show_role_global
                .then(|| config_global_mount_rows_height(&role_global_rows)),
            env_height: inputs
                .show_envs
                .then(|| env_block_height_for_config(inputs.ws_config)),
            show_roles,
            agent_count: inputs.agent_count,
        },
    )
}

#[must_use]
pub fn compute_config_sidebar_scroll_areas(
    area: Rect,
    inputs: &ConfigSidebarInputs<'_>,
    config: &jackin_config::AppConfig,
) -> SidebarScrollAreas {
    let layout = compute_config_sidebar_layout(area, inputs);
    let (global_rows, role_global_rows) =
        crate::services::workspace::split_global_mount_rows(&inputs.global_rows);

    SidebarScrollAreas {
        workspace: SidebarScrollArea {
            area: layout.mounts,
            content_width: workspace_config_mounts_content_width_with_cache(
                inputs.mounts,
                &inputs.mount_info_cache,
            ),
            content_height: workspace_config_mounts_content_height(inputs.mounts),
        },
        global: SidebarScrollArea {
            area: layout.global.unwrap_or(Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 0,
            }),
            content_width: global_mounts_content_width_from_rows(
                &global_rows,
                &inputs.mount_info_cache,
            ),
            content_height: global_mounts_content_height_from_rows(&global_rows),
        },
        role_global: layout.role_global.map(|area| SidebarScrollArea {
            area,
            content_width: global_mounts_content_width_from_rows(
                &role_global_rows,
                &inputs.mount_info_cache,
            ),
            content_height: global_mounts_content_height_from_rows(&role_global_rows),
        }),
        roles: layout.roles.map(|area| SidebarScrollArea {
            area,
            content_width: agents_block_content_width(config.roles.keys().map(String::as_str)),
            content_height: 2 + agents_block_agent_count_for_config(inputs.ws_config, config),
        }),
    }
}

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

fn config_global_mount_rows_height(rows: &[&jackin_config::GlobalMountRow]) -> u16 {
    global_mount_rows_height(rows.iter().map(|row| row.mount.src == row.mount.dst))
}

fn global_mounts_content_width_from_rows(
    rows: &[&jackin_config::GlobalMountRow],
    cache: &MountInfoCache,
) -> usize {
    let mounts: Vec<jackin_config::MountConfig> =
        rows.iter().map(|row| row.mount.clone()).collect();
    global_config_mounts_content_width_with_cache(&mounts, cache)
}

fn global_mounts_content_height_from_rows(rows: &[&jackin_config::GlobalMountRow]) -> usize {
    global_mounts_content_height(rows.iter().map(|row| row.mount.src == row.mount.dst))
}

pub fn clamp_scroll_area_x(area: SidebarScrollArea, value: &mut u16) {
    clamp_scroll_x(area.content_width, scroll_viewport_width(area.area), value);
}

pub fn clamp_scroll_area_y(area: SidebarScrollArea, value: &mut u16) {
    clamp_scroll_x(
        area.content_height,
        scroll_viewport_height(area.area),
        value,
    );
}

pub fn clamp_scroll_area(area: SidebarScrollArea, scroll_x: &mut u16, scroll_y: &mut u16) {
    clamp_scroll_area_x(area, scroll_x);
    clamp_scroll_area_y(area, scroll_y);
}

#[must_use]
pub fn scroll_area_scrollable(area: SidebarScrollArea) -> bool {
    scroll_area_axes(area).any()
}

#[must_use]
pub fn scroll_area_axes(area: SidebarScrollArea) -> ScrollAxes {
    ScrollAxes {
        horizontal: is_scrollable(area.content_width, scroll_viewport_width(area.area)),
        vertical: is_scrollable(area.content_height, scroll_viewport_height(area.area)),
    }
}

fn mount_data_row_count(same_path_rows: impl IntoIterator<Item = bool>) -> Option<usize> {
    let mut saw_row = false;
    let mut lines = 0;
    for same_path in same_path_rows {
        saw_row = true;
        lines += if same_path { 1 } else { 2 };
    }
    saw_row.then_some(lines)
}

fn clamp_scroll_x(content: usize, viewport: usize, value: &mut u16) {
    termrock::scroll::clamp_scroll_offset(content, viewport, value);
}

fn scroll_viewport_width(area: Rect) -> usize {
    termrock::scroll::viewport_width(area)
}

fn scroll_viewport_height(area: Rect) -> usize {
    termrock::scroll::viewport_height(area)
}

fn is_scrollable(content: usize, viewport: usize) -> bool {
    termrock::scroll::is_scrollable(content, viewport)
}

#[cfg(test)]
mod tests;
