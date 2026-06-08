//! Pure workspace-list row sizing helpers.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

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

pub fn clamp_list_names_scroll(list_area: Rect, content_width: usize, scroll_x: &mut u16) {
    let viewport = jackin_tui::components::scrollable_panel::viewport_width(list_area);
    jackin_tui::components::scrollable_panel::clamp_scroll_offset(
        content_width,
        viewport,
        scroll_x,
    );
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
