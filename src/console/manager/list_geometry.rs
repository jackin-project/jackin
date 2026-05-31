//! List-pane geometry used outside the renderer.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::config::AppConfig;
use crate::console::manager::mount_display::{
    global_mounts_content_width_with_cache, workspace_mounts_content_height,
    workspace_mounts_content_width_with_cache,
};
use crate::console::manager::state::{
    ManagerListRow, ManagerState, MountInfoCache, MountScrollFocus, WorkspaceSummary,
};

pub(crate) fn list_names_content_width(state: &ManagerState<'_>, viewport: usize) -> usize {
    let visual_selected = state.visual_selected();
    state
        .visual_rows_vec()
        .iter()
        .enumerate()
        .filter_map(|(visual_idx, row)| {
            row.as_ref().and_then(|row| {
                list_row_width(
                    state,
                    row,
                    visual_idx == visual_selected && state.list_names_focused,
                )
            })
        })
        .max()
        .unwrap_or(0)
        .max(viewport)
}

pub(crate) fn clamp_list_scroll_for_area(
    area: Rect,
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    let left_pct = state.list_split_pct;
    let right_pct = 100u16.saturating_sub(left_pct);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);
    let sidebar_areas = selected_sidebar_scroll_areas(columns[1], state, config, cwd);

    if let Some(areas) = sidebar_areas.as_ref() {
        clamp_scroll_area(areas.workspace, &mut state.list_mounts_scroll_x);
        clamp_scroll_area_y(areas.workspace, &mut state.list_mounts_scroll_y);
        clamp_scroll_area(areas.global, &mut state.list_global_mounts_scroll_x);
        clamp_scroll_area_y(areas.global, &mut state.list_global_mounts_scroll_y);

        if let Some(role_global) = areas.role_global {
            clamp_scroll_area(role_global, &mut state.list_role_global_mounts_scroll_x);
            clamp_scroll_area_y(role_global, &mut state.list_role_global_mounts_scroll_y);
        } else {
            state.list_role_global_mounts_scroll_x = 0;
            state.list_role_global_mounts_scroll_y = 0;
        }

        if let Some(roles) = areas.roles {
            clamp_scroll_area(roles, &mut state.list_roles_scroll_x);
            clamp_scroll_area_y(roles, &mut state.list_roles_scroll_y);
        } else {
            state.list_roles_scroll_x = 0;
            state.list_roles_scroll_y = 0;
        }
    } else {
        state.list_mounts_scroll_x = 0;
        state.list_mounts_scroll_y = 0;
        state.list_global_mounts_scroll_x = 0;
        state.list_global_mounts_scroll_y = 0;
        state.list_role_global_mounts_scroll_x = 0;
        state.list_role_global_mounts_scroll_y = 0;
        state.list_roles_scroll_x = 0;
        state.list_roles_scroll_y = 0;
        state.list_scroll_focus = None;
        if !state.preview_focused {
            state.list_names_focused = true;
        }
    }

    if let Some(focus) = state.list_scroll_focus
        && !focused_block_still_scrollable(focus, sidebar_areas.as_ref())
    {
        state.list_scroll_focus = None;
        state.list_names_focused = true;
    }

    let left_viewport_w = scroll_viewport_width(columns[0]);
    if left_viewport_w == 0 {
        state.list_names_scroll_x = 0;
    } else {
        let name_content_w = list_names_content_width(state, left_viewport_w);
        if is_scrollable(name_content_w, left_viewport_w) {
            let max = max_scroll_offset(name_content_w, left_viewport_w);
            if state.list_names_scroll_x > max {
                state.list_names_scroll_x = max;
            }
        } else {
            state.list_names_scroll_x = 0;
        }
    }
}

pub(crate) fn selected_sidebar_scroll_areas(
    right_pane: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> Option<SidebarScrollAreas> {
    match state.selected_row() {
        ManagerListRow::CurrentDirectory => {
            let cwd_str = cwd.display().to_string();
            let mounts = [crate::workspace::MountConfig {
                src: cwd_str.clone(),
                dst: cwd_str.clone(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }];
            let inputs = sidebar_inputs_for_current_dir(&cwd_str, &mounts, config, state);
            Some(compute_sidebar_scroll_areas(right_pane, &inputs, config))
        }
        ManagerListRow::SavedWorkspace(i) => {
            let summary = state.workspaces.get(i).cloned()?;
            config.workspaces.get(&summary.name)?;
            let inputs = sidebar_inputs_for_workspace(&summary, config, state);
            Some(compute_sidebar_scroll_areas(right_pane, &inputs, config))
        }
        ManagerListRow::NewWorkspace
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::CurrentDirectoryInstance(_) => None,
    }
}

fn clamp_scroll_area(area: SidebarScrollArea, value: &mut u16) {
    clamp_scroll_x(area.content_width, scroll_viewport_width(area.area), value);
}

fn clamp_scroll_area_y(area: SidebarScrollArea, value: &mut u16) {
    clamp_scroll_x(
        area.content_height,
        scroll_viewport_height(area.area),
        value,
    );
}

fn scroll_area_scrollable(area: SidebarScrollArea) -> bool {
    is_scrollable(area.content_width, scroll_viewport_width(area.area))
        || is_scrollable(area.content_height, scroll_viewport_height(area.area))
}

fn focused_block_still_scrollable(
    focus: MountScrollFocus,
    areas: Option<&SidebarScrollAreas>,
) -> bool {
    let Some(areas) = areas else {
        return false;
    };
    match focus {
        MountScrollFocus::Workspace => scroll_area_scrollable(areas.workspace),
        MountScrollFocus::Global => {
            areas.global.area.height > 0 && scroll_area_scrollable(areas.global)
        }
        MountScrollFocus::RoleGlobal => areas.role_global.is_some_and(scroll_area_scrollable),
        MountScrollFocus::Roles => areas.roles.is_some_and(scroll_area_scrollable),
    }
}

fn clamp_scroll_x(content: usize, viewport: usize, value: &mut u16) {
    jackin_tui::components::scrollable_panel::clamp_scroll_offset(content, viewport, value);
}

fn scroll_viewport_width(area: Rect) -> usize {
    jackin_tui::components::scrollable_panel::viewport_width(area)
}

fn scroll_viewport_height(area: Rect) -> usize {
    jackin_tui::components::scrollable_panel::viewport_height(area)
}

fn is_scrollable(content: usize, viewport: usize) -> bool {
    jackin_tui::components::scrollable_panel::is_scrollable(content, viewport)
}

fn max_scroll_offset(content: usize, viewport: usize) -> u16 {
    jackin_tui::components::scrollable_panel::max_offset(content, viewport)
}

fn list_row_width(
    state: &ManagerState<'_>,
    row: &ManagerListRow,
    selected_with_cursor: bool,
) -> Option<usize> {
    match row {
        ManagerListRow::CurrentDirectory => Some(workspace_row_width(
            "Current directory",
            state.has_current_dir_active_instances(),
            selected_with_cursor,
        )),
        ManagerListRow::CurrentDirectoryInstance(inst_idx) => state
            .current_dir_active_instances()
            .get(*inst_idx)
            .map(|entry| instance_row_width(entry, selected_with_cursor)),
        ManagerListRow::SavedWorkspace(i) => state.workspaces.get(*i).map(|ws| {
            workspace_row_width(
                &ws.name,
                state.has_active_instances(*i),
                selected_with_cursor,
            )
        }),
        ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => state
            .workspace_active_instances(*ws_idx)
            .get(*inst_idx)
            .map(|entry| instance_row_width(entry, selected_with_cursor)),
        ManagerListRow::NewWorkspace => Some(workspace_row_width(
            "+ New workspace",
            false,
            selected_with_cursor,
        )),
    }
}

fn workspace_row_width(name: &str, has_instances: bool, selected_with_cursor: bool) -> usize {
    let width = 3 + jackin_tui::display_cols(name);
    let leading_padding = if selected_with_cursor {
        0
    } else if has_instances {
        1
    } else {
        3
    };
    width + leading_padding
}

fn instance_row_width(
    entry: &crate::instance::InstanceIndexEntry,
    selected_with_cursor: bool,
) -> usize {
    let width = 5 + jackin_tui::display_cols(&format!("{}  {}", entry.instance_id, entry.role_key));
    if selected_with_cursor {
        width
    } else {
        width + 5
    }
}

/// Shared inputs for the right-pane sidebar. Saved-workspace rows and the
/// synthetic "Current directory" row both build one of these and feed it
/// through `compute_sidebar_layout` -> `render_sidebar_body` so the panel
/// order, heights, and mouse hit-boxes cannot drift.
pub(crate) struct SidebarInputs<'a> {
    pub workdir: &'a str,
    pub mounts: &'a [crate::workspace::MountConfig],
    pub mount_info_cache: MountInfoCache,
    pub ws_config: Option<&'a crate::workspace::WorkspaceConfig>,
    pub global_rows: Vec<crate::config::GlobalMountRow>,
    pub picker_role_label: String,
    pub instance_count: usize,
    pub instance_expanded: bool,
    pub inline_picker_active: bool,
    pub show_envs: bool,
    pub agent_count: usize,
}

/// Rect for each rendered block. `None` panels are skipped in both render
/// and hit-test.
pub(crate) struct SidebarLayout {
    pub instances: Option<Rect>,
    pub general: Rect,
    pub mounts: Rect,
    pub global: Option<Rect>,
    pub role_global: Option<Rect>,
    pub env: Option<Rect>,
    pub roles: Option<Rect>,
}

#[derive(Clone, Copy)]
pub(crate) struct SidebarScrollArea {
    pub area: Rect,
    pub content_width: usize,
    pub content_height: usize,
}

pub(crate) struct SidebarScrollAreas {
    pub workspace: SidebarScrollArea,
    pub global: SidebarScrollArea,
    pub role_global: Option<SidebarScrollArea>,
    pub roles: Option<SidebarScrollArea>,
}

pub(crate) fn compute_sidebar_layout(area: Rect, inputs: &SidebarInputs<'_>) -> SidebarLayout {
    let (global_rows, role_global_rows) = split_global_mount_rows(&inputs.global_rows);
    let show_global_header = !global_rows.is_empty() || role_global_rows.is_empty();
    let show_global = !inputs.global_rows.is_empty() && show_global_header;
    let show_role_global = !role_global_rows.is_empty();
    let show_roles = !inputs.inline_picker_active;

    let mut constraints = Vec::new();
    if inputs.instance_count > 0 {
        constraints.push(Constraint::Length(COMPACT_INSTANCES_HEIGHT));
    }
    constraints.push(Constraint::Length(3));
    constraints.push(Constraint::Length(mount_block_height(inputs.mounts)));
    if show_global {
        constraints.push(Constraint::Length(global_mount_rows_height(&global_rows)));
    }
    if show_role_global {
        constraints.push(Constraint::Length(global_mount_rows_height(
            &role_global_rows,
        )));
    }
    if inputs.show_envs {
        constraints.push(Constraint::Length(env_block_height(inputs.ws_config)));
    }
    if show_roles {
        constraints.push(Constraint::Length(agents_block_height(inputs.agent_count)));
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    let mut iter = rows.iter().copied();

    SidebarLayout {
        instances: (inputs.instance_count > 0).then(|| iter.next().expect("instances slot")),
        general: iter.next().expect("general slot"),
        mounts: iter.next().expect("mounts slot"),
        global: show_global.then(|| iter.next().expect("global slot")),
        role_global: show_role_global.then(|| iter.next().expect("role-global slot")),
        env: inputs.show_envs.then(|| iter.next().expect("env slot")),
        roles: show_roles.then(|| iter.next().expect("roles slot")),
    }
}

pub(crate) fn compute_sidebar_scroll_areas(
    area: Rect,
    inputs: &SidebarInputs<'_>,
    config: &AppConfig,
) -> SidebarScrollAreas {
    let layout = compute_sidebar_layout(area, inputs);
    let (global_rows, role_global_rows) = split_global_mount_rows(&inputs.global_rows);

    SidebarScrollAreas {
        workspace: SidebarScrollArea {
            area: layout.mounts,
            content_width: workspace_mounts_content_width_with_cache(
                inputs.mounts,
                &inputs.mount_info_cache,
            ),
            content_height: workspace_mounts_content_height(inputs.mounts),
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
            content_width: agents_block_content_width(inputs.ws_config, config),
            content_height: 2 + agents_block_agent_count(inputs.ws_config, config),
        }),
    }
}

pub(crate) fn sidebar_inputs_for_workspace<'a>(
    ws: &'a WorkspaceSummary,
    config: &'a AppConfig,
    state: &ManagerState<'_>,
) -> SidebarInputs<'a> {
    let ws_config = config.workspaces.get(&ws.name);
    let mounts = ws_config.map_or(&[][..], |w| w.mounts.as_slice());
    let picker_role = picker_role_from_state(state);
    let global_rows = global_rows_for_selected_row(state, config);
    let inline_picker_active =
        state.inline_role_picker.is_some() || state.inline_agent_picker.is_some();
    let agent_count = if inline_picker_active {
        0
    } else {
        agents_block_agent_count(ws_config, config)
    };
    SidebarInputs {
        workdir: ws.workdir.as_str(),
        mounts,
        mount_info_cache: state.mount_info_cache.clone(),
        ws_config,
        global_rows,
        picker_role_label: picker_role
            .as_ref()
            .map_or_else(String::new, crate::selector::RoleSelector::key),
        instance_count: workspace_active_count(
            &state.instances,
            Some(ws.name.as_str()),
            &ws.name,
            &ws.workdir,
        ),
        instance_expanded: state
            .workspaces
            .iter()
            .position(|s| s.name == ws.name)
            .is_some_and(|idx| state.is_workspace_expanded(idx)),
        inline_picker_active,
        show_envs: ws_config.is_some_and(workspace_has_any_env),
        agent_count,
    }
}

pub(crate) fn sidebar_inputs_for_current_dir<'a>(
    cwd_str: &'a str,
    mounts: &'a [crate::workspace::MountConfig],
    config: &AppConfig,
    state: &ManagerState<'_>,
) -> SidebarInputs<'a> {
    SidebarInputs {
        workdir: cwd_str,
        mounts,
        mount_info_cache: state.mount_info_cache.clone(),
        ws_config: None,
        global_rows: global_rows_for_selected_row(state, config),
        picker_role_label: String::new(),
        instance_count: workspace_active_count(&state.instances, None, cwd_str, cwd_str),
        instance_expanded: state.current_dir_expanded,
        inline_picker_active: false,
        show_envs: false,
        agent_count: agents_block_agent_count(None, config),
    }
}

pub(crate) fn picker_role_from_state(
    state: &ManagerState<'_>,
) -> Option<crate::selector::RoleSelector> {
    state
        .inline_role_picker
        .as_ref()
        .and_then(|picker| {
            picker
                .list_state
                .selected
                .and_then(|idx| picker.filtered.get(idx).cloned())
        })
        .or_else(|| {
            state
                .inline_agent_picker
                .as_ref()
                .map(|(role, _)| role.clone())
        })
}

pub(crate) fn global_rows_for_selected_row(
    state: &ManagerState<'_>,
    config: &AppConfig,
) -> Vec<crate::config::GlobalMountRow> {
    match state.selected_row() {
        ManagerListRow::CurrentDirectory | ManagerListRow::CurrentDirectoryInstance(_) => {
            global_rows_for(config, None)
        }
        ManagerListRow::SavedWorkspace(i) => {
            let Some(summary) = state.workspaces.get(i) else {
                return Vec::new();
            };
            if !config.workspaces.contains_key(&summary.name) {
                return Vec::new();
            }
            global_rows_for(config, picker_role_from_state(state).as_ref())
        }
        ManagerListRow::NewWorkspace | ManagerListRow::WorkspaceInstance(_, _) => Vec::new(),
    }
}

pub(crate) fn global_rows_for(
    config: &AppConfig,
    picker_role: Option<&crate::selector::RoleSelector>,
) -> Vec<crate::config::GlobalMountRow> {
    picker_role.map_or_else(
        || {
            config
                .list_mount_rows()
                .into_iter()
                .filter(|row| row.scope.is_none())
                .collect()
        },
        |role| config.resolve_mount_rows(role),
    )
}

pub(crate) fn workspace_has_any_env(ws: &crate::workspace::WorkspaceConfig) -> bool {
    !ws.env.is_empty() || ws.roles.values().any(|o| !o.env.is_empty())
}

pub(crate) fn mount_block_height(mounts: &[crate::workspace::MountConfig]) -> u16 {
    let data_rows = if mounts.is_empty() {
        1
    } else {
        mounts
            .iter()
            .map(|mount| if mount.src == mount.dst { 1 } else { 2 })
            .sum()
    };
    (data_rows + 2 + 1).min(12) as u16
}

fn global_mount_rows_height(rows: &[&crate::config::GlobalMountRow]) -> u16 {
    let content_height = if rows.is_empty() {
        1
    } else {
        1 + rows
            .iter()
            .map(|row| if row.mount.src == row.mount.dst { 1 } else { 2 })
            .sum::<usize>()
    };
    (content_height + 2).min(12) as u16
}

pub(crate) fn global_mounts_content_height(mounts: &[crate::workspace::MountConfig]) -> usize {
    if mounts.is_empty() {
        1
    } else {
        1 + mounts
            .iter()
            .map(|mount| if mount.src == mount.dst { 1 } else { 2 })
            .sum::<usize>()
    }
}

pub(crate) fn split_global_mount_rows(
    rows: &[crate::config::GlobalMountRow],
) -> (
    Vec<&crate::config::GlobalMountRow>,
    Vec<&crate::config::GlobalMountRow>,
) {
    rows.iter().partition(|row| row.scope.is_none())
}

fn global_mounts_content_width_from_rows(
    rows: &[&crate::config::GlobalMountRow],
    cache: &MountInfoCache,
) -> usize {
    let mounts: Vec<crate::workspace::MountConfig> =
        rows.iter().map(|row| row.mount.clone()).collect();
    global_mounts_content_width_with_cache(&mounts, cache)
}

fn global_mounts_content_height_from_rows(rows: &[&crate::config::GlobalMountRow]) -> usize {
    let mounts: Vec<crate::workspace::MountConfig> =
        rows.iter().map(|row| row.mount.clone()).collect();
    global_mounts_content_height(&mounts)
}

pub(crate) fn env_block_height(ws_config: Option<&crate::workspace::WorkspaceConfig>) -> u16 {
    let Some(ws) = ws_config else {
        return 2;
    };

    let workspace_keys = ws.env.len();
    let agent_keys: usize = ws.roles.values().map(|o| o.env.len()).sum();
    let total_rows = workspace_keys + agent_keys;
    (total_rows + 2).min(20) as u16
}

pub(crate) fn agents_block_agent_count(
    ws_config: Option<&crate::workspace::WorkspaceConfig>,
    config: &AppConfig,
) -> usize {
    let all_allowed = ws_config.is_none_or(crate::console::manager::agent_allow::allows_all_agents);
    if all_allowed {
        config.roles.len()
    } else {
        ws_config.map_or(0, |w| w.allowed_roles.len())
    }
}

pub(crate) fn agents_block_height(agent_count: usize) -> u16 {
    let agent_rows = agent_count.max(1);
    (2 + 1 + 1 + agent_rows).min(14) as u16
}

pub(crate) fn agents_block_content_width(
    _ws_config: Option<&crate::workspace::WorkspaceConfig>,
    config: &AppConfig,
) -> usize {
    config.roles.keys().map(|k| k.len() + 4).max().unwrap_or(0)
}

/// Fixed height of the compact running-instances badge (borders + 1 text line).
pub(crate) const COMPACT_INSTANCES_HEIGHT: u16 = 3;

pub(crate) fn workspace_active_count(
    instances: &[crate::instance::InstanceIndexEntry],
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
) -> usize {
    let query = crate::instance::InstanceQuery {
        workspace_name,
        workspace_label,
        workdir,
        role_key: None,
        agent_runtime: None,
    };
    crate::console::manager::state::active_instances_matching(instances, query).count()
}
