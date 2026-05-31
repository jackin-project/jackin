//! List-pane geometry used outside the renderer.

use crate::console::manager::state::{ManagerListRow, ManagerState};

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
