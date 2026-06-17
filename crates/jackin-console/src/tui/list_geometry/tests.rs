//! Tests for `list_geometry`.
use super::*;

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
