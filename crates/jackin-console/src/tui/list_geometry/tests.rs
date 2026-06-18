//! Tests for `list_geometry`.
use super::*;
use crate::tui::screens::workspaces::model::ManagerListRow;

#[test]
fn workspace_rows_account_for_cursor_and_instances() {
    assert_eq!(workspace_row_width("abc", false, true), 6);
    assert_eq!(workspace_row_width("abc", true, false), 7);
    assert_eq!(workspace_row_width("abc", false, false), 9);
}

#[test]
fn list_names_width_keeps_viewport_as_floor() {
    assert_eq!(list_names_content_width([3, 12, 5], 20), 20);
    assert_eq!(list_names_content_width([3, 12, 5], 8), 12);
    assert_eq!(list_names_content_width([], 8), 8);
}

#[test]
fn scroll_axes_helpers_report_overflow() {
    assert_eq!(
        horizontal_scroll_axes(true, 20, Rect::new(0, 0, 10, 3)),
        ScrollAxes {
            horizontal: true,
            vertical: false
        }
    );
    assert_eq!(
        horizontal_scroll_axes(false, 20, Rect::new(0, 0, 10, 3)),
        ScrollAxes::none()
    );
    assert_eq!(
        vertical_scroll_axes(10, Rect::new(0, 0, 20, 3)),
        ScrollAxes {
            horizontal: false,
            vertical: true
        }
    );
    assert_eq!(
        list_names_scroll_axes(20, Rect::new(0, 0, 10, 3)),
        ScrollAxes {
            horizontal: true,
            vertical: false
        }
    );
}

#[test]
fn split_list_columns_allocates_preview_remainder() {
    let columns = split_list_columns(Rect::new(0, 0, 100, 10), 35);
    assert_eq!(columns.names.width, 35);
    assert_eq!(columns.preview.width, 65);
}

#[test]
fn instance_rows_leave_indent_when_not_selected() {
    assert_eq!(instance_row_width("i-1", "role", true), 14);
    assert_eq!(instance_row_width("i-1", "role", false), 19);
}

#[test]
fn manager_list_row_width_routes_all_row_kinds() {
    assert_eq!(
        manager_list_row_width(
            ManagerListRow::CurrentDirectory,
            false,
            true,
            |_| None,
            |_| None,
            |_, _| None,
        ),
        Some(workspace_row_width(
            crate::tui::screens::workspaces::view::current_directory_workspace_title(),
            true,
            false,
        ))
    );
    assert_eq!(
        manager_list_row_width(
            ManagerListRow::SavedWorkspace(2),
            true,
            false,
            |_| None,
            |idx| (idx == 2).then(|| ("ws".to_owned(), true)),
            |_, _| None,
        ),
        Some(workspace_row_width("ws", true, true))
    );
    assert_eq!(
        manager_list_row_width(
            ManagerListRow::WorkspaceInstance(1, 3),
            false,
            false,
            |_| None,
            |_| None,
            |ws, inst| (ws == 1 && inst == 3).then(|| ("i-1".to_owned(), "role".to_owned())),
        ),
        Some(instance_row_width("i-1", "role", false))
    );
    assert_eq!(
        manager_list_row_width(
            ManagerListRow::CurrentDirectoryInstance(4),
            true,
            false,
            |inst| (inst == 4).then(|| ("i-cwd".to_owned(), "agent".to_owned())),
            |_| None,
            |_, _| None,
        ),
        Some(instance_row_width("i-cwd", "agent", true))
    );
    assert_eq!(
        manager_list_row_width(
            ManagerListRow::NewWorkspace,
            false,
            false,
            |_| None,
            |_| None,
            |_, _| None,
        ),
        Some(workspace_row_width(
            crate::tui::screens::workspaces::view::new_workspace_list_label(),
            false,
            false,
        ))
    );
}

#[test]
fn manager_list_row_width_returns_none_for_missing_backing_row() {
    assert_eq!(
        manager_list_row_width(
            ManagerListRow::SavedWorkspace(9),
            false,
            false,
            |_| None,
            |_| None,
            |_, _| None,
        ),
        None
    );
}
