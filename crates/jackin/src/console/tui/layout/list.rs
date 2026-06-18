//! List-pane geometry used outside the renderer.

use ratatui::layout::Rect;

use crate::console::tui::state::{ManagerListRow, ManagerState, WorkspaceSummary};
use jackin_config::AppConfig;
use jackin_console::tui::list_geometry::{instance_row_width, workspace_row_width};
use jackin_console::tui::screens::workspaces::view::{
    current_directory_workspace_title, new_workspace_list_label,
};
pub(crate) use jackin_console::tui::sidebar_layout::{
    ConfigSidebarInputs as SidebarInputs, ConfigSidebarSelectionInputs, SidebarLayout,
    SidebarScrollAreas,
};
use jackin_console::tui::update::{list_pre_render_focus_plan, list_pre_render_scroll_reset_plan};

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
                        visual_idx == visual_selected && state.list_names_focused(),
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
    let columns =
        jackin_console::tui::list_geometry::split_list_columns(area, state.list_split_pct);
    let sidebar_areas = selected_sidebar_scroll_areas(columns.preview, state, config, cwd);
    let sidebar_available = sidebar_areas.is_some();
    let focused_block_scrollable = state.list_scroll_focus().is_none_or(|focus| {
        jackin_console::tui::sidebar_layout::focused_mount_scroll_area_still_scrollable(
            focus,
            sidebar_areas.as_ref(),
        )
    });
    let role_global_available = sidebar_areas
        .as_ref()
        .and_then(|areas| areas.role_global)
        .is_some();
    let roles_available = sidebar_areas
        .as_ref()
        .and_then(|areas| areas.roles)
        .is_some();

    if let Some(areas) = sidebar_areas.as_ref() {
        jackin_console::tui::sidebar_layout::clamp_scroll_area(
            areas.workspace,
            &mut state.list_mounts_scroll_x,
            &mut state.list_mounts_scroll_y,
        );
        jackin_console::tui::sidebar_layout::clamp_scroll_area(
            areas.global,
            &mut state.list_global_mounts_scroll_x,
            &mut state.list_global_mounts_scroll_y,
        );

        if let Some(role_global) = areas.role_global {
            jackin_console::tui::sidebar_layout::clamp_scroll_area(
                role_global,
                &mut state.list_role_global_mounts_scroll_x,
                &mut state.list_role_global_mounts_scroll_y,
            );
        }

        if let Some(roles) = areas.roles {
            jackin_console::tui::sidebar_layout::clamp_scroll_area(
                roles,
                &mut state.list_roles_scroll_x,
                &mut state.list_roles_scroll_y,
            );
        }
    }

    let reset_plan = list_pre_render_scroll_reset_plan(
        sidebar_available,
        role_global_available,
        roles_available,
    );
    if reset_plan.reset_workspace {
        state.list_mounts_scroll_x = 0;
        state.list_mounts_scroll_y = 0;
    }
    if reset_plan.reset_global {
        state.list_global_mounts_scroll_x = 0;
        state.list_global_mounts_scroll_y = 0;
    }
    if reset_plan.reset_role_global {
        state.list_role_global_mounts_scroll_x = 0;
        state.list_role_global_mounts_scroll_y = 0;
    }
    if reset_plan.reset_roles {
        state.list_roles_scroll_x = 0;
        state.list_roles_scroll_y = 0;
    }

    let focus_plan = list_pre_render_focus_plan(
        state.list_scroll_focus(),
        state.list_names_focused(),
        state.preview_focused,
        sidebar_available,
        focused_block_scrollable,
    );
    state.set_list_scroll_focus(focus_plan.list_scroll_focus);
    state.set_list_names_focused(focus_plan.list_names_focused);

    let left_viewport_w = jackin_console::tui::layout::scroll_viewport_width(columns.names);
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
            let mounts = [jackin_console::services::workspace::current_dir_mount_config(&cwd_str)];
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

fn list_row_width(
    state: &ManagerState<'_>,
    row: &ManagerListRow,
    selected_with_cursor: bool,
) -> Option<usize> {
    match row {
        ManagerListRow::CurrentDirectory => Some(workspace_row_width(
            current_directory_workspace_title(),
            state.has_current_dir_active_instances(),
            selected_with_cursor,
        )),
        ManagerListRow::CurrentDirectoryInstance(inst_idx) => state
            .current_dir_active_instances()
            .get(*inst_idx)
            .map(|entry| {
                instance_row_width(&entry.instance_id, &entry.role_key, selected_with_cursor)
            }),
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
            .map(|entry| {
                instance_row_width(&entry.instance_id, &entry.role_key, selected_with_cursor)
            }),
        ManagerListRow::NewWorkspace => Some(workspace_row_width(
            new_workspace_list_label(),
            false,
            selected_with_cursor,
        )),
    }
}

pub(crate) fn compute_sidebar_layout(area: Rect, inputs: &SidebarInputs<'_>) -> SidebarLayout {
    jackin_console::tui::sidebar_layout::compute_config_sidebar_layout(area, inputs)
}

pub(crate) fn compute_sidebar_scroll_areas(
    area: Rect,
    inputs: &SidebarInputs<'_>,
    config: &AppConfig,
) -> SidebarScrollAreas {
    jackin_console::tui::sidebar_layout::compute_config_sidebar_scroll_areas(area, inputs, config)
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
        ConfigSidebarSelectionInputs {
            workdir: ws.workdir.as_str(),
            mounts,
            mount_info_cache: state.mount_info_cache.clone(),
            ws_config,
            global_rows: global_rows_for_selected_row(state, config),
            picker_role_label: picker_role
                .as_ref()
                .map_or_else(String::new, jackin_core::RoleSelector::key),
            instance_count: workspace_active_count(
                &state.instances,
                Some(ws.name.as_str()),
                ws.name.as_str(),
                ws.workdir.as_str(),
            ),
            instance_expanded: state
                .workspaces
                .iter()
                .position(|s| s.name == ws.name)
                .is_some_and(|idx| state.is_workspace_expanded(idx)),
            inline_picker_active,
            show_envs: ws_config.is_some_and(|ws| {
                let workspace_keys = ws.env.len();
                let agent_keys = ws.roles.values().map(|role| role.env.len()).sum();
                jackin_console::tui::sidebar_layout::workspace_has_any_env(
                    workspace_keys,
                    agent_keys,
                )
            }),
        },
        config,
    )
}

pub(crate) fn sidebar_inputs_for_current_dir<'a>(
    cwd_str: &'a str,
    mounts: &'a [jackin_config::MountConfig],
    config: &'a AppConfig,
    state: &ManagerState<'_>,
) -> SidebarInputs<'a> {
    sidebar_inputs_for_selection(
        ConfigSidebarSelectionInputs {
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
        },
        config,
    )
}

fn sidebar_inputs_for_selection<'a>(
    selection: ConfigSidebarSelectionInputs<'a>,
    config: &'a AppConfig,
) -> SidebarInputs<'a> {
    jackin_console::tui::sidebar_layout::config_sidebar_inputs_for_selection(selection, config)
}

pub(crate) fn picker_role_from_state(
    state: &ManagerState<'_>,
) -> Option<jackin_core::RoleSelector> {
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
) -> Vec<jackin_config::GlobalMountRow> {
    match state.selected_row() {
        ManagerListRow::CurrentDirectory | ManagerListRow::CurrentDirectoryInstance(_) => {
            jackin_console::services::workspace::global_rows_for_picker(config, None)
        }
        ManagerListRow::SavedWorkspace(i) => {
            let Some(summary) = state.workspaces.get(i) else {
                return Vec::new();
            };
            if !config.workspaces.contains_key(&summary.name) {
                return Vec::new();
            }
            jackin_console::services::workspace::global_rows_for_picker(
                config,
                picker_role_from_state(state).as_ref(),
            )
        }
        ManagerListRow::NewWorkspace | ManagerListRow::WorkspaceInstance(_, _) => Vec::new(),
    }
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
