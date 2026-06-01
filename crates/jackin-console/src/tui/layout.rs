pub const LIST_HEADER_HEIGHT: u16 = 2;
pub const LIST_FOOTER_HEIGHT: u16 = 2;
pub const SCREEN_HEADER_HEIGHT: u16 = 3;
pub const TAB_STRIP_HEIGHT: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarAxis {
    Horizontal,
    Vertical,
}

/// Compute the seam column for a percentage split and total terminal width.
#[must_use]
pub const fn split_seam_column(pct: u16, width: u16) -> u16 {
    width.saturating_mul(pct) / 100
}

/// Return `(left_x, left_width, right_x, right_width)` using Ratatui's
/// percentage layout arithmetic.
#[must_use]
pub fn horizontal_split_pane_dims(pct: u16, total_width: u16) -> (u16, u16, u16, u16) {
    let right_pct = 100u16.saturating_sub(pct);
    let cols = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage(pct),
            ratatui::layout::Constraint::Percentage(right_pct),
        ])
        .split(ratatui::layout::Rect {
            x: 0,
            y: 0,
            width: total_width,
            height: 1,
        });
    (cols[0].x, cols[0].width, cols[1].x, cols[1].width)
}

/// Derive a new split percentage from a drag anchor and current mouse column.
#[must_use]
pub fn split_pct_from_drag(anchor_pct: u16, anchor_x: u16, mouse_col: u16, width: u16) -> u16 {
    let delta_cols = i32::from(mouse_col) - i32::from(anchor_x);
    let delta_pct = delta_cols * 100 / i32::from(width.max(1));
    let candidate = i32::from(anchor_pct) + delta_pct;
    let bounded = candidate.clamp(0, 100);
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    {
        bounded as u16
    }
}

#[must_use]
pub fn scrollbar_drag_offset(
    axis: ScrollbarAxis,
    area: ratatui::layout::Rect,
    content_len: usize,
    pointer_col: u16,
    pointer_row: u16,
) -> Option<u16> {
    use jackin_tui::components::scrollable_panel::{
        horizontal_scrollbar_area, is_scrollable, scrollbar_offset_for_track_position,
        vertical_scrollbar_area, viewport_height, viewport_width,
    };

    let (viewport, scrollbar, track_len, track_position) = match axis {
        ScrollbarAxis::Horizontal => {
            let scrollbar = horizontal_scrollbar_area(area);
            (
                viewport_width(area),
                scrollbar,
                scrollbar.width,
                pointer_col.saturating_sub(scrollbar.x),
            )
        }
        ScrollbarAxis::Vertical => {
            let scrollbar = vertical_scrollbar_area(area);
            (
                viewport_height(area),
                scrollbar,
                scrollbar.height,
                pointer_row.saturating_sub(scrollbar.y),
            )
        }
    };
    if !is_scrollable(content_len, viewport)
        || !point_in_rect(pointer_col, pointer_row, scrollbar)
    {
        return None;
    }
    Some(scrollbar_offset_for_track_position(
        content_len,
        viewport,
        usize::from(track_len),
        usize::from(track_position),
    ))
}

#[must_use]
pub const fn tabbed_content_area(
    term_size: ratatui::layout::Rect,
    cached_footer_h: u16,
) -> ratatui::layout::Rect {
    ratatui::layout::Rect {
        x: 0,
        y: SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT,
        width: term_size.width,
        height: term_size
            .height
            .saturating_sub(SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT + cached_footer_h),
    }
}

#[must_use]
pub fn tab_cell_at_position(row: u16, col: u16, labels: &[&str]) -> Option<usize> {
    if row < SCREEN_HEADER_HEIGHT || row >= SCREEN_HEADER_HEIGHT.saturating_add(TAB_STRIP_HEIGHT) {
        return None;
    }
    let cells: Vec<(&str, bool)> = labels.iter().map(|label| (*label, false)).collect();
    let laid = jackin_tui::lay_out_tabs(&cells, 0);
    jackin_tui::tab_at_column(&laid, col)
}

#[must_use]
pub const fn point_in_rect(col: u16, row: u16, area: ratatui::layout::Rect) -> bool {
    col >= area.x
        && col < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

/// Like a centered percent-width rect, but takes a fixed row height.
#[must_use]
pub fn centered_rect_fixed(
    outer: ratatui::layout::Rect,
    pct_w: u16,
    rows: u16,
) -> ratatui::layout::Rect {
    let w = outer.width * pct_w / 100;
    let h = rows.min(outer.height);
    ratatui::layout::Rect {
        x: outer.x + outer.width.saturating_sub(w) / 2,
        y: outer.y + outer.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SCREEN_HEADER_HEIGHT, ScrollbarAxis, TAB_STRIP_HEIGHT, horizontal_split_pane_dims,
        point_in_rect, scrollbar_drag_offset, split_pct_from_drag, split_seam_column,
        tab_cell_at_position, tabbed_content_area,
    };
    use ratatui::layout::Rect;

    #[test]
    fn split_seam_column_uses_saturating_percent_math() {
        assert_eq!(split_seam_column(30, 100), 30);
        assert_eq!(split_seam_column(30, 0), 0);
    }

    #[test]
    fn horizontal_split_pane_dims_match_ratatui_percentage_layout() {
        assert_eq!(horizontal_split_pane_dims(30, 100), (0, 30, 30, 70));
        assert_eq!(horizontal_split_pane_dims(33, 101), (0, 33, 33, 68));
    }

    #[test]
    fn split_pct_from_drag_handles_signed_delta_and_bounds() {
        assert_eq!(split_pct_from_drag(30, 30, 50, 100), 50);
        assert_eq!(split_pct_from_drag(30, 30, 0, 100), 0);
        assert_eq!(split_pct_from_drag(80, 80, 200, 100), 100);
    }

    #[test]
    fn scrollbar_drag_offset_maps_pointer_to_scroll_offset() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 5,
        };
        assert_eq!(
            scrollbar_drag_offset(ScrollbarAxis::Horizontal, area, 100, 10, 4),
            Some(44)
        );
        assert_eq!(
            scrollbar_drag_offset(ScrollbarAxis::Vertical, area, 100, 19, 2),
            Some(48)
        );
        assert_eq!(
            scrollbar_drag_offset(ScrollbarAxis::Horizontal, area, 10, 10, 4),
            None
        );
    }

    #[test]
    fn point_in_rect_uses_half_open_edges() {
        let area = Rect {
            x: 2,
            y: 3,
            width: 5,
            height: 4,
        };

        assert!(point_in_rect(2, 3, area));
        assert!(point_in_rect(6, 6, area));
        assert!(!point_in_rect(7, 3, area));
        assert!(!point_in_rect(2, 7, area));
    }

    #[test]
    fn tabbed_content_area_excludes_header_tabs_and_footer() {
        let term = Rect {
            x: 0,
            y: 0,
            width: 120,
            height: 40,
        };
        assert_eq!(
            tabbed_content_area(term, 3),
            Rect {
                x: 0,
                y: SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT,
                width: 120,
                height: 32,
            }
        );
    }

    #[test]
    fn tab_cell_at_position_uses_shared_tab_layout() {
        let labels = ["General", "Mounts", "Auth"];

        assert_eq!(
            tab_cell_at_position(SCREEN_HEADER_HEIGHT, 1, &labels),
            Some(0)
        );
        assert_eq!(
            tab_cell_at_position(SCREEN_HEADER_HEIGHT + 1, 11, &labels),
            Some(1)
        );
        assert_eq!(tab_cell_at_position(SCREEN_HEADER_HEIGHT - 1, 1, &labels), None);
        assert_eq!(
            tab_cell_at_position(SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT, 1, &labels),
            None
        );
    }
}
