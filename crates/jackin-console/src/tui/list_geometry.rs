//! Pure workspace-list row sizing helpers.

#[must_use]
pub fn list_names_content_width(
    row_widths: impl IntoIterator<Item = usize>,
    viewport: usize,
) -> usize {
    row_widths.into_iter().max().unwrap_or(0).max(viewport)
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
mod tests {
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
    fn instance_rows_leave_indent_when_not_selected() {
        assert_eq!(instance_row_width("i-1", "role", true), 14);
        assert_eq!(instance_row_width("i-1", "role", false), 19);
    }
}
