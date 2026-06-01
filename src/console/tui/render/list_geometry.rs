//! List-pane geometry used outside the renderer.

use ratatui::layout::Rect;

use crate::config::AppConfig;
use crate::console::tui::render::mount_display::{
    global_mounts_content_width_with_cache, workspace_mounts_content_height,
    workspace_mounts_content_width_with_cache,
};
use crate::console::tui::state::{
    ManagerListRow, ManagerState, MountInfoCache, MountScrollFocus, WorkspaceSummary,
};
pub(crate) use jackin_console::tui::sidebar_layout::{
    SidebarLayout, SidebarScrollArea, SidebarScrollAreas, SidebarScrollFocus,
};

pub(crate) fn list_names_content_width(state: &ManagerState<'_>, viewport: usize) -> usize {
    let visual_selected = state.visual_selected();
    jackin_console::tui::list_geometry::list_names_content_width(
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
            }),
        viewport,
    )
}

pub(crate) fn clamp_list_scroll_for_area(
    area: Rect,
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    let columns = jackin_console::tui::list_geometry::split_list_columns(
        area,
        state.list_split_pct,
    );
    let sidebar_areas = selected_sidebar_scroll_areas(columns.preview, state, config, cwd);

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

    let left_viewport_w = scroll_viewport_width(columns.names);
    let name_content_w = list_names_content_width(state, left_viewport_w);
    jackin_console::tui::list_geometry::clamp_list_names_scroll(
        columns.names,
        name_content_w,
        &mut state.list_names_scroll_x,
    );
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
            let mounts = [crate::console::domain::current_dir_mount_config(&cwd_str)];
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
    jackin_console::tui::sidebar_layout::clamp_scroll_area_x(area, value);
}

fn clamp_scroll_area_y(area: SidebarScrollArea, value: &mut u16) {
    jackin_console::tui::sidebar_layout::clamp_scroll_area_y(area, value);
}

fn sidebar_scroll_focus(focus: MountScrollFocus) -> SidebarScrollFocus {
    match focus {
        MountScrollFocus::Workspace => SidebarScrollFocus::Workspace,
        MountScrollFocus::Global => SidebarScrollFocus::Global,
        MountScrollFocus::RoleGlobal => SidebarScrollFocus::RoleGlobal,
        MountScrollFocus::Roles => SidebarScrollFocus::Roles,
    }
}

fn focused_block_still_scrollable(
    focus: MountScrollFocus,
    areas: Option<&SidebarScrollAreas>,
) -> bool {
    jackin_console::tui::sidebar_layout::focused_scroll_area_still_scrollable(
        sidebar_scroll_focus(focus),
        areas,
    )
}

fn scroll_viewport_width(area: Rect) -> usize {
    jackin_tui::components::scrollable_panel::viewport_width(area)
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
    jackin_console::tui::list_geometry::workspace_row_width(
        name,
        has_instances,
        selected_with_cursor,
    )
}

fn instance_row_width(
    entry: &crate::instance::InstanceIndexEntry,
    selected_with_cursor: bool,
) -> usize {
    jackin_console::tui::list_geometry::instance_row_width(
        &entry.instance_id,
        &entry.role_key,
        selected_with_cursor,
    )
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

pub(crate) fn compute_sidebar_layout(area: Rect, inputs: &SidebarInputs<'_>) -> SidebarLayout {
    let (global_rows, role_global_rows) = split_global_mount_rows(&inputs.global_rows);
    let show_global_header = !global_rows.is_empty() || role_global_rows.is_empty();
    let show_global = !inputs.global_rows.is_empty() && show_global_header;
    let show_role_global = !role_global_rows.is_empty();
    let show_roles = !inputs.inline_picker_active;

    jackin_console::tui::sidebar_layout::compute_sidebar_layout(
        area,
        jackin_console::tui::sidebar_layout::SidebarLayoutMetrics {
            instance_count: inputs.instance_count,
            workspace_mount_height: mount_block_height(inputs.mounts),
            global_mount_height: show_global.then(|| global_mount_rows_height(&global_rows)),
            role_global_mount_height: show_role_global
                .then(|| global_mount_rows_height(&role_global_rows)),
            env_height: inputs.show_envs.then(|| env_block_height(inputs.ws_config)),
            show_roles,
            agent_count: inputs.agent_count,
        },
    )
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
    let inline_picker_active =
        state.inline_role_picker.is_some() || state.inline_agent_picker.is_some();
    sidebar_inputs_for_selection(
        SidebarSelectionInputs {
            workspace_name: Some(ws.name.as_str()),
            workspace_label: ws.name.as_str(),
            workdir: ws.workdir.as_str(),
            mounts,
            ws_config,
            picker_role_label: picker_role
                .as_ref()
                .map_or_else(String::new, crate::selector::RoleSelector::key),
            instance_expanded: state
                .workspaces
                .iter()
                .position(|s| s.name == ws.name)
                .is_some_and(|idx| state.is_workspace_expanded(idx)),
            inline_picker_active,
            show_envs: ws_config.is_some_and(workspace_has_any_env),
        },
        config,
        state,
    )
}

pub(crate) fn sidebar_inputs_for_current_dir<'a>(
    cwd_str: &'a str,
    mounts: &'a [crate::workspace::MountConfig],
    config: &'a AppConfig,
    state: &ManagerState<'_>,
) -> SidebarInputs<'a> {
    sidebar_inputs_for_selection(
        SidebarSelectionInputs {
            workspace_name: None,
            workspace_label: cwd_str,
            workdir: cwd_str,
            mounts,
            ws_config: None,
            picker_role_label: String::new(),
            instance_expanded: state.current_dir_expanded,
            inline_picker_active: false,
            show_envs: false,
        },
        config,
        state,
    )
}

struct SidebarSelectionInputs<'a> {
    workspace_name: Option<&'a str>,
    workspace_label: &'a str,
    workdir: &'a str,
    mounts: &'a [crate::workspace::MountConfig],
    ws_config: Option<&'a crate::workspace::WorkspaceConfig>,
    picker_role_label: String,
    instance_expanded: bool,
    inline_picker_active: bool,
    show_envs: bool,
}

fn sidebar_inputs_for_selection<'a>(
    selection: SidebarSelectionInputs<'a>,
    config: &'a AppConfig,
    state: &ManagerState<'_>,
) -> SidebarInputs<'a> {
    let agent_count = if selection.inline_picker_active {
        0
    } else {
        agents_block_agent_count(selection.ws_config, config)
    };
    SidebarInputs {
        workdir: selection.workdir,
        mounts: selection.mounts,
        mount_info_cache: state.mount_info_cache.clone(),
        ws_config: selection.ws_config,
        global_rows: global_rows_for_selected_row(state, config),
        picker_role_label: selection.picker_role_label,
        instance_count: workspace_active_count(
            &state.instances,
            selection.workspace_name,
            selection.workspace_label,
            selection.workdir,
        ),
        instance_expanded: selection.instance_expanded,
        inline_picker_active: selection.inline_picker_active,
        show_envs: selection.show_envs,
        agent_count,
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
    let workspace_keys = ws.env.len();
    let agent_keys: usize = ws.roles.values().map(|o| o.env.len()).sum();
    jackin_console::tui::sidebar_layout::workspace_has_any_env(workspace_keys, agent_keys)
}

pub(crate) fn mount_block_height(mounts: &[crate::workspace::MountConfig]) -> u16 {
    jackin_console::tui::sidebar_layout::mount_block_height(
        mounts.iter().map(|mount| mount.src == mount.dst),
    )
}

fn global_mount_rows_height(rows: &[&crate::config::GlobalMountRow]) -> u16 {
    jackin_console::tui::sidebar_layout::global_mount_rows_height(
        rows.iter().map(|row| row.mount.src == row.mount.dst),
    )
}

pub(crate) fn global_mounts_content_height(mounts: &[crate::workspace::MountConfig]) -> usize {
    jackin_console::tui::sidebar_layout::global_mounts_content_height(
        mounts.iter().map(|mount| mount.src == mount.dst),
    )
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
    jackin_console::tui::sidebar_layout::env_block_height(workspace_keys, agent_keys)
}

pub(crate) fn agents_block_agent_count(
    ws_config: Option<&crate::workspace::WorkspaceConfig>,
    config: &AppConfig,
) -> usize {
    let all_allowed = ws_config.is_none_or(jackin_console::workspace::allows_all_agents);
    let allowed_role_count = ws_config.map_or(0, |w| w.allowed_roles.len());
    jackin_console::tui::sidebar_layout::agents_block_agent_count(
        all_allowed,
        config.roles.len(),
        allowed_role_count,
    )
}

pub(crate) fn agents_block_content_width(
    _ws_config: Option<&crate::workspace::WorkspaceConfig>,
    config: &AppConfig,
) -> usize {
    jackin_console::tui::sidebar_layout::agents_block_content_width(
        config.roles.keys().map(String::as_str),
    )
}

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
    crate::console::tui::state::active_instances_matching(instances, query).count()
}
