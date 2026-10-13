// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace list sidebar rendering.

use super::{list_name_lines, render_account_picker_sidebar};
use ratatui::{Frame, layout::Rect};

use crate::tui::screens::workspaces::view::{
    WorkspaceListNamesRenderFacts, WorkspaceSidebarFacts, WorkspaceSidebarPlan,
    current_directory_workspace_title, render_agent_picker_sidebar, render_list_names_block,
    render_role_picker_sidebar, workspace_list_names_render_plan, workspace_sidebar_owns_focus,
    workspace_sidebar_plan,
};
use crate::tui::state::ManagerState;

pub fn render_list_sidebar(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    let sidebar_owns_focus =
        workspace_sidebar_owns_focus(state.list_names_focused(), state.list_modal.is_some());
    match workspace_sidebar_plan(WorkspaceSidebarFacts {
        inline_account_picker_open: state.inline_account_picker.is_some(),
        launch_account_picker_open: state.launch_account_picker.is_some(),
        inline_new_session_picker_open: state.inline_new_session_picker.is_some(),
        inline_agent_picker_open: state.inline_agent_picker.is_some(),
        inline_role_picker_open: state.inline_role_picker.is_some(),
    }) {
        WorkspaceSidebarPlan::InlineAccountPicker => {
            if let Some(picker) = state.inline_account_picker.as_ref() {
                let short_id = jackin_core::instance_id_from_container_base(&picker.context)
                    .unwrap_or(picker.context.as_str());
                render_account_picker_sidebar(
                    frame,
                    area,
                    Some(short_id),
                    picker.providers(),
                    picker.selected(),
                    sidebar_owns_focus,
                );
            }
        }
        WorkspaceSidebarPlan::LaunchAccountPicker => {
            if let Some(picker) = state.launch_account_picker.as_ref() {
                render_account_picker_sidebar(
                    frame,
                    area,
                    None,
                    picker.providers(),
                    picker.selected(),
                    sidebar_owns_focus,
                );
            }
        }
        WorkspaceSidebarPlan::InlineNewSessionPicker => {
            if let Some((container, picker, _providers)) = state.inline_new_session_picker.as_ref()
            {
                let short_id =
                    jackin_core::instance_id_from_container_base(container).unwrap_or(container);
                render_agent_picker_sidebar(frame, area, short_id, picker, sidebar_owns_focus);
            }
        }
        WorkspaceSidebarPlan::InlineAgentPicker => {
            if let Some((role, picker)) = state.inline_agent_picker.as_ref() {
                render_agent_picker_sidebar(frame, area, &role.key(), picker, sidebar_owns_focus);
            }
        }
        WorkspaceSidebarPlan::InlineRolePicker => {
            if let Some(picker) = state.inline_role_picker.as_ref() {
                let title = state
                    .selected_workspace_summary()
                    .map_or(current_directory_workspace_title(), |summary| {
                        summary.name.as_str()
                    });
                render_role_picker_sidebar(frame, area, title, picker, sidebar_owns_focus);
            }
        }
        WorkspaceSidebarPlan::ListNames => {
            render_list_names_sidebar(frame, area, state, sidebar_owns_focus);
        }
    }
}

pub(crate) fn render_list_names_sidebar(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ManagerState<'_>,
    sidebar_owns_focus: bool,
) {
    let visual_rows = state.visual_rows_vec();
    let plan = workspace_list_names_render_plan(WorkspaceListNamesRenderFacts {
        area,
        selected_index: state.visual_selected(),
        row_count: visual_rows.len(),
        scroll_y: state.list_names_scroll.offset_y(),
    });
    let (list_lines, content_width) =
        list_name_lines(state, plan.viewport_width, sidebar_owns_focus);
    render_list_names_block(
        frame,
        area,
        list_lines,
        content_width,
        sidebar_owns_focus,
        state.list_names_scroll.offset_x(),
        plan.follow_scroll_y,
    );
}
