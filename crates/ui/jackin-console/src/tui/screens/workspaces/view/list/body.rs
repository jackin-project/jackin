// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace list body rendering.

use super::{
    render_current_dir_details_pane, render_details_pane, render_instance_details_pane,
    render_list_sidebar,
};
use ratatui::{Frame, layout::Rect, text::Line};

use crate::tui::screens::workspaces::view::{
    InstanceRowLabel, WorkspaceListDisplayRowsFacts, WorkspacePreviewPanePlan,
    list_name_lines as workspace_list_name_lines, render_sentinel_description_pane,
    workspace_list_display_rows, workspace_preview_pane_plan,
};
use crate::tui::state::ManagerState;
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
