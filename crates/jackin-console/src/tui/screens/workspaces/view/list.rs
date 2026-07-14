// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Root-console workspace-list display adapters.

use ratatui::{Frame, layout::Rect, text::Line};

use crate::tui::layout::list::{
    SidebarInputs, SidebarLayout, compute_sidebar_layout, sidebar_inputs_for_current_dir,
    sidebar_inputs_for_workspace,
};
use crate::tui::screens::workspaces::view::{
    InstanceRowLabel, WorkspaceInstanceLivePaneFacts, WorkspaceInstanceLiveTabFacts,
    WorkspaceInstancePane, WorkspaceInstancePaneContent, WorkspaceInstanceSessionRow,
    WorkspaceListDisplayRowsFacts, WorkspaceListNamesRenderFacts, WorkspacePreviewPanePlan,
    WorkspaceSidebarFacts, WorkspaceSidebarPlan, current_directory_workspace_title,
    global_mounts_title, list_name_lines as workspace_list_name_lines, render_agent_picker_sidebar,
    render_compact_instances_summary, render_config_mounts_subpanel, render_config_roles_subpanel,
    render_environments_subpanel, render_general_subpanel, render_global_mount_rows_section,
    render_instance_details_pane as render_workspace_instance_details_pane,
    render_list_names_block, render_provider_picker_sidebar as render_provider_picker_sidebar_view,
    render_role_picker_sidebar, render_sentinel_description_pane, role_global_mounts_title,
    workspace_env_rows, workspace_instance_live_content, workspace_instance_pane,
    workspace_instance_session_content, workspace_list_display_rows,
    workspace_list_names_render_plan, workspace_preview_pane_plan, workspace_sidebar_owns_focus,
    workspace_sidebar_plan,
};
use crate::tui::state::{ManagerState, MountScrollFocus, WorkspaceSummary};
use jackin_config::AppConfig;

pub fn render_list_body(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    // See ManagerListRow docs for row layout.
    // Split driven by `state.list_split_pct` (default 30), adjustable via
    // mouse-drag on the seam column. Keeps the right pane visible on every
    // row. Row-specific right-pane renderers:
    //   CurrentDirectory  → current-dir details
    //   SavedWorkspace(i) → saved-workspace details
    //   NewWorkspace      → description-of-what-a-workspace-is pane
    let columns = crate::tui::list_geometry::split_list_columns(area, state.list_split_pct);
    let list_area = columns.names;

    match workspace_preview_pane_plan(state.selected_row()) {
        WorkspacePreviewPanePlan::CurrentDirectory => {
            render_current_dir_details_pane(frame, columns.preview, cwd, config, state);
        }
        WorkspacePreviewPanePlan::NewWorkspace => {
            render_sentinel_description_pane(frame, columns.preview);
        }
        WorkspacePreviewPanePlan::SavedWorkspace(i) => {
            if let Some(ws) = state.workspaces.get(i).cloned() {
                render_details_pane(frame, columns.preview, &ws, config, state);
            }
        }
        WorkspacePreviewPanePlan::Instance {
            workspace_idx,
            instance_idx,
        } => {
            let instances = match workspace_idx {
                Some(ws_idx) => state.workspace_visible_instances(ws_idx),
                None => state.current_dir_visible_instances(),
            };
            if let Some(entry) = instances.get(instance_idx).copied() {
                let sessions = state.sessions_for_instance(&entry.container_base);
                let session_load_error = state.has_session_load_error(&entry.container_base);
                let snapshot = state.snapshot_for_instance(&entry.container_base);
                let selected_pane = if state.preview_focused {
                    state
                        .preview_selected_pane(&entry.container_base)
                        .map(|(_, id)| id)
                } else {
                    None
                };
                render_instance_details_pane(
                    frame,
                    columns.preview,
                    entry,
                    sessions,
                    session_load_error,
                    snapshot,
                    selected_pane,
                    state.preview_focused,
                );
            }
        }
    }

    render_list_sidebar(frame, list_area, state);
}

pub fn list_name_lines(
    state: &ManagerState<'_>,
    viewport: usize,
    show_cursor: bool,
) -> (Vec<Line<'static>>, usize) {
    let visual_rows = state.visual_rows_vec();
    let visual_selected = state.visual_selected();
    let hovered_row = state.hovered_list_row();
    let display_rows = workspace_list_display_rows(
        WorkspaceListDisplayRowsFacts {
            visual_rows: &visual_rows,
            visual_selected,
            hovered_row,
            current_dir_expanded: state.current_dir_expanded,
            current_dir_has_instances: state.has_current_dir_visible_instances(),
        },
        |inst_idx| {
            state
                .current_dir_visible_instances()
                .get(inst_idx)
                .map(|entry| InstanceRowLabel {
                    instance_id: entry.instance_id.clone(),
                    role_key: entry.role_key.clone(),
                    status: entry.status,
                })
        },
        |idx| {
            state.workspaces.get(idx).map(|ws| {
                (
                    ws.name.clone(),
                    state.is_workspace_expanded(idx),
                    state.has_visible_instances(idx),
                )
            })
        },
        |ws_idx, inst_idx| {
            state
                .workspace_visible_instances(ws_idx)
                .get(inst_idx)
                .map(|entry| InstanceRowLabel {
                    instance_id: entry.instance_id.clone(),
                    role_key: entry.role_key.clone(),
                    status: entry.status,
                })
        },
    );
    workspace_list_name_lines(&display_rows, viewport, show_cursor)
}

pub fn instance_details_pane(
    entry: &jackin_core::InstanceIndexEntry,
    sessions: &[jackin_core::SessionRecord],
    session_load_error: bool,
    snapshot: Option<&jackin_protocol::InstanceSnapshot>,
    selected_pane: Option<u64>,
    preview_focused: bool,
) -> WorkspaceInstancePane {
    workspace_instance_pane(
        entry.instance_id.clone(),
        preview_focused,
        instance_details_content(sessions, session_load_error, snapshot, selected_pane),
    )
}

fn instance_details_content(
    sessions: &[jackin_core::SessionRecord],
    session_load_error: bool,
    snapshot: Option<&jackin_protocol::InstanceSnapshot>,
    selected_pane: Option<u64>,
) -> WorkspaceInstancePaneContent {
    if let Some(snapshot) = snapshot {
        return workspace_instance_live_content(
            snapshot.active_tab as usize,
            selected_pane,
            snapshot
                .tabs
                .iter()
                .map(|tab| WorkspaceInstanceLiveTabFacts {
                    label: tab.label.clone(),
                    focused_pane: tab.focused_pane,
                    panes: tab
                        .panes
                        .iter()
                        .map(|pane| WorkspaceInstanceLivePaneFacts {
                            session_id: pane.session_id,
                            label: pane.label.clone(),
                            agent: pane.agent.clone(),
                            state_label: pane.state.label().to_owned(),
                        })
                        .collect(),
                }),
        );
    }
    workspace_instance_session_content(
        session_load_error,
        sessions.iter().map(|session| WorkspaceInstanceSessionRow {
            name: session.tmux_name.clone(),
            agent_runtime: session.agent_runtime.clone(),
        }),
    )
}

pub fn render_list_sidebar(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    let sidebar_owns_focus =
        workspace_sidebar_owns_focus(state.list_names_focused(), state.list_modal.is_some());
    match workspace_sidebar_plan(WorkspaceSidebarFacts {
        inline_provider_picker_open: state.inline_provider_picker.is_some(),
        launch_provider_picker_open: state.launch_provider_picker.is_some(),
        inline_new_session_picker_open: state.inline_new_session_picker.is_some(),
        inline_agent_picker_open: state.inline_agent_picker.is_some(),
        inline_role_picker_open: state.inline_role_picker.is_some(),
    }) {
        WorkspaceSidebarPlan::InlineProviderPicker => {
            if let Some(picker) = state.inline_provider_picker.as_ref() {
                let short_id = jackin_core::instance_id_from_container_base(&picker.context)
                    .unwrap_or(picker.context.as_str());
                render_provider_picker_sidebar(
                    frame,
                    area,
                    Some(short_id),
                    picker.providers(),
                    picker.selected(),
                    sidebar_owns_focus,
                );
            }
        }
        WorkspaceSidebarPlan::LaunchProviderPicker => {
            if let Some(picker) = state.launch_provider_picker.as_ref() {
                render_provider_picker_sidebar(
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

fn render_list_names_sidebar(
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
        scroll_y: state.list_names_scroll_y,
    });
    let (list_lines, content_width) =
        list_name_lines(state, plan.viewport_width, sidebar_owns_focus);
    render_list_names_block(
        frame,
        area,
        list_lines,
        content_width,
        sidebar_owns_focus,
        state.list_names_scroll_x,
        plan.follow_scroll_y,
    );
}

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

pub fn render_provider_picker_sidebar(
    frame: &mut Frame<'_>,
    area: Rect,
    container_id: Option<&str>,
    providers: &[jackin_protocol::Provider],
    selected: usize,
    focused: bool,
) {
    let labels = providers
        .iter()
        .map(|provider| provider.label().to_owned())
        .collect();
    render_provider_picker_sidebar_view(frame, area, container_id, labels, selected, focused);
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
        state.list_mounts_scroll_x,
        state.list_mounts_scroll_y,
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
                state.list_global_mounts_scroll_x,
                state.list_global_mounts_scroll_y,
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
                state.list_role_global_mounts_scroll_x,
                state.list_role_global_mounts_scroll_y,
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
            state.list_roles_scroll_x,
            state.list_roles_scroll_y,
            roles_focused,
        );
    }
}

#[cfg(test)]
mod tests;
