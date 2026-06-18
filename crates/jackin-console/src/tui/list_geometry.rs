//! Pure workspace-list row sizing helpers.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use jackin_tui::components::ScrollAxes;

use crate::tui::screens::workspaces::model::ManagerListRow;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListColumns {
    pub names: Rect,
    pub preview: Rect,
}

#[must_use]
pub fn split_list_columns(area: Rect, left_pct: u16) -> ListColumns {
    let right_pct = 100u16.saturating_sub(left_pct);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);
    ListColumns {
        names: columns[0],
        preview: columns[1],
    }
}

#[must_use]
pub fn list_names_content_width(
    row_widths: impl IntoIterator<Item = usize>,
    viewport: usize,
) -> usize {
    row_widths.into_iter().max().unwrap_or(0).max(viewport)
}

#[must_use]
pub fn manager_list_row_width(
    row: ManagerListRow,
    selected_with_cursor: bool,
    current_dir_has_instances: bool,
    current_dir_instance: impl FnOnce(usize) -> Option<(String, String)>,
    saved_workspace: impl FnOnce(usize) -> Option<(String, bool)>,
    workspace_instance: impl FnOnce(usize, usize) -> Option<(String, String)>,
) -> Option<usize> {
    match row {
        ManagerListRow::CurrentDirectory => Some(workspace_row_width(
            crate::tui::screens::workspaces::view::current_directory_workspace_title(),
            current_dir_has_instances,
            selected_with_cursor,
        )),
        ManagerListRow::CurrentDirectoryInstance(inst_idx) => {
            current_dir_instance(inst_idx).map(|(instance_id, role_key)| {
                instance_row_width(instance_id, &role_key, selected_with_cursor)
            })
        }
        ManagerListRow::SavedWorkspace(idx) => saved_workspace(idx).map(|(name, has_instances)| {
            workspace_row_width(&name, has_instances, selected_with_cursor)
        }),
        ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => workspace_instance(ws_idx, inst_idx)
            .map(|(instance_id, role_key)| {
                instance_row_width(instance_id, &role_key, selected_with_cursor)
            }),
        ManagerListRow::NewWorkspace => Some(workspace_row_width(
            crate::tui::screens::workspaces::view::new_workspace_list_label(),
            false,
            selected_with_cursor,
        )),
    }
}

pub fn clamp_list_names_scroll(list_area: Rect, content_width: usize, scroll_x: &mut u16) {
    let viewport = jackin_tui::components::scrollable_panel::viewport_width(list_area);
    jackin_tui::components::scrollable_panel::clamp_scroll_offset(
        content_width,
        viewport,
        scroll_x,
    );
}

#[must_use]
pub fn horizontal_scroll_axes(has_content: bool, content_width: usize, area: Rect) -> ScrollAxes {
    if !has_content {
        return ScrollAxes::none();
    }
    let viewport = jackin_tui::components::scrollable_panel::viewport_width(area);
    ScrollAxes {
        horizontal: jackin_tui::components::scrollable_panel::is_scrollable(
            content_width,
            viewport,
        ),
        vertical: false,
    }
}

#[must_use]
pub fn vertical_scroll_axes(content_height: usize, area: Rect) -> ScrollAxes {
    let viewport = jackin_tui::components::scrollable_panel::viewport_height(area);
    ScrollAxes {
        horizontal: false,
        vertical: jackin_tui::components::scrollable_panel::is_scrollable(content_height, viewport),
    }
}

#[must_use]
pub fn list_names_scroll_axes(content_width: usize, list_area: Rect) -> ScrollAxes {
    let viewport = crate::tui::layout::scroll_viewport_width(list_area);
    ScrollAxes {
        horizontal: jackin_tui::components::scrollable_panel::max_offset(content_width, viewport)
            > 0,
        vertical: false,
    }
}

#[must_use]
pub fn workspace_row_width(name: &str, has_instances: bool, selected_with_cursor: bool) -> usize {
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

#[must_use]
pub fn instance_row_width(
    instance_id: impl std::fmt::Display,
    role_key: &str,
    selected_with_cursor: bool,
) -> usize {
    let width = 5 + jackin_tui::display_cols(&format!("{instance_id}  {role_key}"));
    if selected_with_cursor {
        width
    } else {
        width + 5
    }
}

#[cfg(test)]
mod tests;
