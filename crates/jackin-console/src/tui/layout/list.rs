// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! List-pane geometry used outside the renderer.

use ratatui::layout::Rect;

pub use crate::tui::sidebar_layout::{
    ConfigSidebarInputs as SidebarInputs, ConfigSidebarSelectionInputs, GlobalMountRowsSelection,
    SelectedSidebarTarget, SidebarInstanceFacts, SidebarInstanceQuery, SidebarLayout,
    SidebarScrollAreas, inline_picker_active as sidebar_inline_picker_active,
};
use crate::tui::state::{ManagerState, WorkspaceSummary};
use crate::tui::update::{list_pre_render_facts_from_scroll_areas, list_pre_render_plan};
use jackin_config::AppConfig;

pub fn list_names_content_width(state: &ManagerState<'_>, viewport: usize) -> usize {
    let visual_rows = state.visual_rows_vec();
    crate::tui::list_geometry::manager_list_names_content_width(
        crate::tui::list_geometry::ManagerListNamesContentWidthFacts {
            visual_rows: &visual_rows,
            visual_selected: state.visual_selected(),
            list_names_focused: state.list_names_focused(),
            current_dir_has_instances: state.has_current_dir_visible_instances(),
            viewport,
        },
        |inst_idx| {
            state
                .current_dir_visible_instances()
                .get(inst_idx)
                .map(|entry| {
                    (
                        entry.instance_id.clone(),
                        entry.role_key.clone(),
                        entry.status,
                    )
                })
        },
        |idx| {
            state
                .workspaces
                .get(idx)
                .map(|ws| (ws.name.clone(), state.has_visible_instances(idx)))
        },
        |ws_idx, inst_idx| {
            state
                .workspace_visible_instances(ws_idx)
                .get(inst_idx)
                .map(|entry| {
                    (
                        entry.instance_id.clone(),
                        entry.role_key.clone(),
                        entry.status,
                    )
                })
        },
    )
}

pub fn clamp_list_scroll_for_area(
    area: Rect,
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    let columns = crate::tui::list_geometry::split_list_columns(area, state.list_split_pct);
    let sidebar_areas = selected_sidebar_scroll_areas(columns.preview, state, config, cwd);

    if let Some(areas) = sidebar_areas.as_ref() {
        crate::tui::sidebar_layout::clamp_scroll_area(
            areas.workspace,
            &mut state.list_mounts_scroll_x,
            &mut state.list_mounts_scroll_y,
        );
        crate::tui::sidebar_layout::clamp_scroll_area(
            areas.global,
            &mut state.list_global_mounts_scroll_x,
            &mut state.list_global_mounts_scroll_y,
        );

        if let Some(role_global) = areas.role_global {
            crate::tui::sidebar_layout::clamp_scroll_area(
                role_global,
                &mut state.list_role_global_mounts_scroll_x,
                &mut state.list_role_global_mounts_scroll_y,
            );
        }

        if let Some(roles) = areas.roles {
            crate::tui::sidebar_layout::clamp_scroll_area(
                roles,
                &mut state.list_roles_scroll_x,
                &mut state.list_roles_scroll_y,
            );
        }
    }

    let pre_render_plan = list_pre_render_plan(list_pre_render_facts_from_scroll_areas(
        state.list_scroll_focus(),
        state.list_names_focused(),
        state.preview_focused,
        sidebar_areas.as_ref(),
    ));
    let reset_plan = pre_render_plan.scroll_reset;
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

    let focus_plan = pre_render_plan.focus;
    state.set_list_scroll_focus(focus_plan.list_scroll_focus);
    state.set_list_names_focused(focus_plan.list_names_focused);

    let left_viewport_w = crate::tui::layout::scroll_viewport_width(columns.names);
    let name_content_w = list_names_content_width(state, left_viewport_w);
    crate::tui::list_geometry::clamp_list_names_scroll(
        columns.names,
        name_content_w,
        &mut state.list_names_scroll_x,
    );
}

pub fn selected_sidebar_scroll_areas(
    right_pane: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> Option<SidebarScrollAreas> {
    match crate::tui::sidebar_layout::selected_sidebar_target(state.selected_row())? {
        SelectedSidebarTarget::CurrentDirectory => {
            let cwd_str = cwd.display().to_string();
            let mounts = [crate::services::workspace::current_dir_mount_config(
                &cwd_str,
            )];
            let inputs = sidebar_inputs_for_current_dir(&cwd_str, &mounts, config, state);
            Some(compute_sidebar_scroll_areas(right_pane, &inputs, config))
        }
        SelectedSidebarTarget::SavedWorkspace(i) => {
            let summary = state.workspaces.get(i).cloned()?;
            config.workspaces.get(&summary.name)?;
            let inputs = sidebar_inputs_for_workspace(&summary, config, state);
            Some(compute_sidebar_scroll_areas(right_pane, &inputs, config))
        }
    }
}

pub fn compute_sidebar_layout(area: Rect, inputs: &SidebarInputs<'_>) -> SidebarLayout {
    crate::tui::sidebar_layout::compute_config_sidebar_layout(area, inputs)
}

pub fn compute_sidebar_scroll_areas(
    area: Rect,
    inputs: &SidebarInputs<'_>,
    config: &AppConfig,
) -> SidebarScrollAreas {
    crate::tui::sidebar_layout::compute_config_sidebar_scroll_areas(area, inputs, config)
}

pub fn sidebar_inputs_for_workspace<'a>(
    ws: &'a WorkspaceSummary,
    config: &'a AppConfig,
    state: &ManagerState<'_>,
) -> SidebarInputs<'a> {
    let ws_config = config.workspaces.get(&ws.name);
    let mounts = ws_config.map_or(&[][..], |w| w.mounts.as_slice());
    let picker_role = picker_role_from_state(state);
    let inline_picker_active = sidebar_inline_picker_active(
        state.inline_role_picker.is_some(),
        state.inline_agent_picker.is_some(),
    );
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
                crate::tui::sidebar_layout::workspace_has_any_env(workspace_keys, agent_keys)
            }),
        },
        config,
    )
}

pub fn sidebar_inputs_for_current_dir<'a>(
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
    crate::tui::sidebar_layout::config_sidebar_inputs_for_selection(selection, config)
}

pub fn picker_role_from_state(state: &ManagerState<'_>) -> Option<jackin_core::RoleSelector> {
    crate::tui::sidebar_layout::inline_picker_role(
        state.inline_role_picker.as_ref().and_then(|picker| {
            picker
                .list_state
                .selected()
                .copied()
                .and_then(|idx| picker.filtered.get(idx).cloned())
        }),
        state
            .inline_agent_picker
            .as_ref()
            .map(|(role, _)| role.clone()),
    )
}

pub fn global_rows_for_selected_row(
    state: &ManagerState<'_>,
    config: &AppConfig,
) -> Vec<jackin_config::GlobalMountRow> {
    match crate::tui::sidebar_layout::global_mount_rows_selection(
        state.selected_row(),
        |idx| {
            state
                .workspaces
                .get(idx)
                .is_some_and(|summary| config.workspaces.contains_key(&summary.name))
        },
        picker_role_from_state(state),
    ) {
        GlobalMountRowsSelection::CurrentDirectory => {
            crate::services::workspace::global_rows_for_picker(config, None)
        }
        GlobalMountRowsSelection::SavedWorkspace { picker_role } => {
            crate::services::workspace::global_rows_for_picker(config, picker_role.as_ref())
        }
        GlobalMountRowsSelection::None => Vec::new(),
    }
}

pub fn workspace_active_count(
    instances: &[jackin_core::InstanceIndexEntry],
    workspace_name: Option<&str>,
    workspace_label: &str,
    workdir: &str,
) -> usize {
    let query = SidebarInstanceQuery {
        workspace_name,
        workspace_label,
        workdir,
    };
    crate::tui::sidebar_layout::sidebar_active_instance_count(
        instances.iter().map(|entry| SidebarInstanceFacts {
            workspace_name: entry.workspace_name.as_deref(),
            workspace_label: entry.workspace_label.as_str(),
            workdir: entry.workdir.as_str(),
            active: matches!(
                entry.status,
                jackin_core::InstanceStatus::Active | jackin_core::InstanceStatus::Running
            ),
        }),
        query,
    )
}
