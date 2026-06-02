//! Layout utilities shared across console screens: header/content/footer
//! heights, seam geometry, and scrollbar axis constants.
//!
//! Not responsible for: split percentage state (see `split`) or rendering
//! any widget.

pub const LIST_HEADER_HEIGHT: u16 = 2;
pub const LIST_FOOTER_HEIGHT: u16 = 2;
/// Minimum terminal width where the list/details seam is draggable.
pub const MIN_DRAGGABLE_WIDTH: u16 = 40;
pub const MOUSE_HORIZONTAL_SCROLL_STEP: u16 = 1;
pub const MOUSE_VERTICAL_SCROLL_STEP: i16 = 1;
pub const SCREEN_HEADER_HEIGHT: u16 = 3;
/// Half-width of the list/details seam hit-region.
pub const SEAM_HIT_SLACK: u16 = 1;
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

/// `true` when `column` is inside the list/details seam hit-region.
#[must_use]
pub const fn near_seam(column: u16, seam_x: u16) -> bool {
    let lo = seam_x.saturating_sub(SEAM_HIT_SLACK);
    let hi = seam_x.saturating_add(SEAM_HIT_SLACK);
    column >= lo && column <= hi
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

#[must_use]
pub const fn list_body_area(term_size: ratatui::layout::Rect) -> ratatui::layout::Rect {
    ratatui::layout::Rect {
        x: 0,
        y: LIST_HEADER_HEIGHT,
        width: term_size.width,
        height: term_size
            .height
            .saturating_sub(LIST_HEADER_HEIGHT + LIST_FOOTER_HEIGHT),
    }
}

/// Return the visual list row index under a mouse position, excluding the
/// left pane border, seam column, top border, and bottom border.
#[must_use]
pub const fn list_content_visual_index_at(
    col: u16,
    row: u16,
    term_size: ratatui::layout::Rect,
    seam_x: u16,
) -> Option<usize> {
    if col == 0 || col >= seam_x {
        return None;
    }
    let content_top = LIST_HEADER_HEIGHT + 1;
    let body_end = term_size.height.saturating_sub(LIST_FOOTER_HEIGHT);
    let content_bottom = body_end.saturating_sub(1);
    if row < content_top || row >= content_bottom {
        return None;
    }
    Some((row - content_top) as usize)
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
    if !is_scrollable(content_len, viewport) || !point_in_rect(pointer_col, pointer_row, scrollbar)
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

pub fn apply_horizontal_scroll(
    value: &mut u16,
    delta: i16,
    area: ratatui::layout::Rect,
    content_width: usize,
) {
    use jackin_tui::components::scrollable_panel::apply_scroll_delta;

    apply_scroll_delta(value, delta, scroll_viewport_width(area), content_width);
}

pub fn apply_vertical_scroll(
    value: &mut u16,
    delta: i16,
    area: ratatui::layout::Rect,
    content_height: usize,
) {
    use jackin_tui::components::scrollable_panel::apply_scroll_delta;

    apply_scroll_delta(value, delta, scroll_viewport_height(area), content_height);
}

#[must_use]
pub const fn scroll_viewport_width(area: ratatui::layout::Rect) -> usize {
    jackin_tui::components::scrollable_panel::viewport_width(area)
}

#[must_use]
pub const fn scroll_viewport_height(area: ratatui::layout::Rect) -> usize {
    jackin_tui::components::scrollable_panel::viewport_height(area)
}

#[must_use]
pub const fn is_horizontally_scrollable(area: ratatui::layout::Rect, content_width: usize) -> bool {
    jackin_tui::components::scrollable_panel::is_scrollable(
        content_width,
        scroll_viewport_width(area),
    )
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
        MIN_DRAGGABLE_WIDTH, MOUSE_HORIZONTAL_SCROLL_STEP, MOUSE_VERTICAL_SCROLL_STEP,
        SCREEN_HEADER_HEIGHT, SEAM_HIT_SLACK, ScrollbarAxis, TAB_STRIP_HEIGHT,
        apply_horizontal_scroll, apply_vertical_scroll, horizontal_split_pane_dims,
        is_horizontally_scrollable, list_body_area, list_content_visual_index_at, near_seam,
        point_in_rect, scroll_viewport_height, scroll_viewport_width, scrollbar_drag_offset,
        split_pct_from_drag, split_seam_column, tab_cell_at_position, tabbed_content_area,
    };
    use ratatui::layout::Rect;

    #[test]
    fn split_seam_column_uses_saturating_percent_math() {
        assert_eq!(split_seam_column(30, 100), 30);
        assert_eq!(split_seam_column(30, 0), 0);
    }

    #[test]
    fn near_seam_uses_one_column_hit_slack() {
        assert_eq!(MIN_DRAGGABLE_WIDTH, 40);
        assert_eq!(SEAM_HIT_SLACK, 1);
        assert_eq!(MOUSE_HORIZONTAL_SCROLL_STEP, 1);
        assert_eq!(MOUSE_VERTICAL_SCROLL_STEP, 1);

        assert!(near_seam(29, 30));
        assert!(near_seam(30, 30));
        assert!(near_seam(31, 30));
        assert!(!near_seam(28, 30));
        assert!(!near_seam(32, 30));
    }

    #[test]
    fn horizontal_split_pane_dims_match_ratatui_percentage_layout() {
        assert_eq!(horizontal_split_pane_dims(30, 100), (0, 30, 30, 70));
        assert_eq!(horizontal_split_pane_dims(33, 101), (0, 33, 33, 68));
    }

    #[test]
    fn list_body_area_reserves_header_and_footer() {
        assert_eq!(
            list_body_area(Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 24,
            }),
            Rect {
                x: 0,
                y: 2,
                width: 100,
                height: 20,
            }
        );
    }

    #[test]
    fn list_content_visual_index_excludes_borders() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 24,
        };
        assert_eq!(list_content_visual_index_at(1, 3, area, 30), Some(0));
        assert_eq!(list_content_visual_index_at(1, 4, area, 30), Some(1));
        assert_eq!(list_content_visual_index_at(0, 3, area, 30), None);
        assert_eq!(list_content_visual_index_at(30, 3, area, 30), None);
        assert_eq!(list_content_visual_index_at(1, 2, area, 30), None);
        assert_eq!(list_content_visual_index_at(1, 21, area, 30), None);
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
    fn scroll_apply_helpers_use_scrollable_panel_viewports() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 6,
        };

        let mut horizontal = 0;
        apply_horizontal_scroll(&mut horizontal, 20, area, 40);
        assert_eq!(horizontal, 20);
        apply_horizontal_scroll(&mut horizontal, 20, area, 40);
        assert_eq!(horizontal, 32);

        let mut vertical = 0;
        apply_vertical_scroll(&mut vertical, 20, area, 40);
        assert_eq!(vertical, 20);
        apply_vertical_scroll(&mut vertical, 20, area, 40);
        assert_eq!(vertical, 36);

        assert_eq!(scroll_viewport_width(area), 8);
        assert_eq!(scroll_viewport_height(area), 4);
        assert!(is_horizontally_scrollable(area, 40));
        assert!(!is_horizontally_scrollable(area, 8));
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
        assert_eq!(
            tab_cell_at_position(SCREEN_HEADER_HEIGHT - 1, 1, &labels),
            None
        );
        assert_eq!(
            tab_cell_at_position(SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT, 1, &labels),
            None
        );
    }
}
