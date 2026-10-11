// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Details panes and sidebar body.

use super::instance_details_pane;
use crate::tui::layout::list::{
    SidebarInputs, SidebarLayout, compute_sidebar_layout, sidebar_inputs_for_current_dir,
    sidebar_inputs_for_workspace,
};
use crate::tui::screens::workspaces::view::{
    global_mounts_title, render_account_picker_sidebar as render_account_picker_sidebar_view,
    render_compact_instances_summary, render_config_mounts_subpanel, render_config_roles_subpanel,
    render_environments_subpanel, render_general_subpanel, render_global_mount_rows_section,
    render_instance_details_pane as render_workspace_instance_details_pane,
    role_global_mounts_title, workspace_env_rows,
};
use crate::tui::state::{ManagerState, MountScrollFocus, WorkspaceSummary};
use jackin_config::AppConfig;
use ratatui::{Frame, layout::Rect};

pub fn render_details_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    ws: &WorkspaceSummary,
    config: &AppConfig,
    state: &ManagerState<'_>,
) {
    let inputs = sidebar_inputs_for_workspace(ws, config, state);
    let layout = compute_sidebar_layout(area, &inputs);
    render_sidebar_body(frame, &layout, &inputs, config, state);
}

pub fn render_current_dir_details_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    cwd: &std::path::Path,
    config: &AppConfig,
    state: &ManagerState<'_>,
) {
    let cwd_str = cwd.display().to_string();
    let mounts = [crate::services::workspace::current_dir_mount_config(
        &cwd_str,
    )];
    let inputs = sidebar_inputs_for_current_dir(&cwd_str, &mounts, config, state);
    let layout = compute_sidebar_layout(area, &inputs);
    render_sidebar_body(frame, &layout, &inputs, config, state);
}

#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn render_instance_details_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    entry: &jackin_core::InstanceIndexEntry,
    sessions: &[jackin_core::SessionRecord],
    session_load_error: bool,
    snapshot: Option<&jackin_protocol::InstanceSnapshot>,
    selected_pane: Option<u64>,
    preview_focused: bool,
) {
    let pane = instance_details_pane(
        entry,
        sessions,
        session_load_error,
        snapshot,
        selected_pane,
        preview_focused,
    );
    render_workspace_instance_details_pane(frame, area, &pane);
}

pub fn render_account_picker_sidebar(
    frame: &mut Frame<'_>,
    area: Rect,
    container_id: Option<&str>,
    providers: &[crate::services::launch::AccountChoice],
    selected: usize,
    focused: bool,
) {
    let labels = providers
        .iter()
        .map(crate::services::launch::AccountChoice::label)
        .collect();
    render_account_picker_sidebar_view(frame, area, container_id, labels, selected, focused);
}

pub fn render_sidebar_body(
    frame: &mut Frame<'_>,
    layout: &SidebarLayout,
    inputs: &SidebarInputs<'_>,
    config: &AppConfig,
    state: &ManagerState<'_>,
) {
    if let Some(area) = layout.instances {
        render_compact_instances_summary(
            frame,
            area,
            inputs.instance_count,
            inputs.instance_expanded,
        );
    }
    render_general_subpanel(
        frame,
        layout.general,
        &jackin_core::shorten_home(inputs.workdir),
    );
    let ws_focused = state.list_scroll_focus() == Some(MountScrollFocus::Workspace);
    render_config_mounts_subpanel(
        frame,
        layout.mounts,
        inputs.mounts,
        &inputs.mount_info_cache,
        state.list_mounts_scroll.offset_x(),
        state.list_mounts_scroll.offset_y(),
        ws_focused,
    );
    if layout.global.is_some() || layout.role_global.is_some() {
        let global_focused = state.list_scroll_focus();
        let (global_rows, role_global_rows) =
            crate::services::workspace::split_global_mount_rows(&inputs.global_rows);
        if let Some(area) = layout.global {
            render_global_mount_rows_section(
                frame,
                area,
                global_mounts_title(),
                &global_rows,
                &inputs.mount_info_cache,
                state.list_global_mounts_scroll.offset_x(),
                state.list_global_mounts_scroll.offset_y(),
                global_focused == Some(MountScrollFocus::Global),
            );
        }
        if let Some(area) = layout.role_global {
            let title = role_global_mounts_title(&inputs.picker_role_label);
            render_global_mount_rows_section(
                frame,
                area,
                &title,
                &role_global_rows,
                &inputs.mount_info_cache,
                state.list_role_global_mounts_scroll.offset_x(),
                state.list_role_global_mounts_scroll.offset_y(),
                global_focused == Some(MountScrollFocus::RoleGlobal),
            );
        }
    }
    if let Some(area) = layout.env {
        render_environments_subpanel(frame, area, workspace_env_rows(inputs.ws_config));
    }
    if let Some(area) = layout.roles {
        let roles_focused = state.list_scroll_focus() == Some(MountScrollFocus::Roles);
        render_config_roles_subpanel(
            frame,
            area,
            inputs.ws_config,
            config,
            state.list_roles_scroll.offset_x(),
            state.list_roles_scroll.offset_y(),
            roles_focused,
        );
    }
}
