// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "documented residual allow; prefer expect when site is lint-true"
    )]
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

pub fn apply_scrollbar_drag(
    axis: ScrollbarAxis,
    value: &mut u16,
    area: ratatui::layout::Rect,
    content_len: usize,
    pointer_col: u16,
    pointer_row: u16,
) -> bool {
    let Some(offset) = scrollbar_drag_offset(axis, area, content_len, pointer_col, pointer_row)
    else {
        return false;
    };
    *value = offset;
    true
}

pub fn scroll_selection_at_position(
    area: ratatui::layout::Rect,
    col: u16,
    row: u16,
    delta: i16,
    mut scroll_selection: impl FnMut(i16) -> bool,
) -> bool {
    if !point_in_rect(col, row, area) {
        return false;
    }
    let _changed = scroll_selection(delta);
    true
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
    let cells: Vec<(&str, bool)> = labels.iter().map(|label| (*label, false)).collect();
    jackin_tui::components::TabStrip::new(&cells).hit_index_at(
        ratatui::layout::Rect {
            x: 0,
            y: SCREEN_HEADER_HEIGHT,
            width: u16::MAX,
            height: TAB_STRIP_HEIGHT,
        },
        col,
        row,
    )
}

#[must_use]
pub fn tab_hover_index_at_position(row: u16, col: u16, labels: &[&str]) -> Option<usize> {
    let cells: Vec<(&str, bool)> = labels.iter().map(|label| (*label, false)).collect();
    let mut tracker = jackin_tui::components::HoverTracker::new();
    jackin_tui::components::TabStrip::new(&cells).register_hover_targets(
        &mut tracker,
        ratatui::layout::Rect {
            x: 0,
            y: SCREEN_HEADER_HEIGHT,
            width: u16::MAX,
            height: TAB_STRIP_HEIGHT,
        },
        |idx| idx,
    );
    tracker.hovered(col, row).copied()
}

#[must_use]
pub const fn point_in_rect(col: u16, row: u16, area: ratatui::layout::Rect) -> bool {
    col >= area.x
        && col < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

/// Map a pointer inside a bordered scrollable content block to the visual row
/// exposed by that block's scrolled content.
#[must_use]
pub fn bordered_content_hit_at_position<T>(
    area: ratatui::layout::Rect,
    col: u16,
    row: u16,
    scroll_y: u16,
    mut hit: impl FnMut(usize) -> Option<T>,
) -> Option<T> {
    let content_x = area.x.saturating_add(1);
    let content_y = area.y.saturating_add(1);
    let content_width = area.width.saturating_sub(2);
    let content_height = area.height.saturating_sub(2);
    if content_width == 0
        || content_height == 0
        || col < content_x
        || col >= content_x.saturating_add(content_width)
        || row < content_y
        || row >= content_y.saturating_add(content_height)
    {
        return None;
    }
    let visual_row = usize::from(row.saturating_sub(content_y)) + usize::from(scroll_y);
    hit(visual_row)
}

/// Apply a horizontal scroll delta, returning whether `value` actually moved
/// (i.e. the content could scroll in that direction). Callers use the result to
/// decide whether a wheel gesture had any effect on this panel.
pub fn apply_horizontal_scroll(
    value: &mut u16,
    delta: i16,
    area: ratatui::layout::Rect,
    content_width: usize,
) -> bool {
    use jackin_tui::components::scrollable_panel::apply_scroll_delta;

    let before = *value;
    apply_scroll_delta(value, delta, scroll_viewport_width(area), content_width);
    *value != before
}

/// Apply a vertical scroll delta, returning whether `value` actually moved.
pub fn apply_vertical_scroll(
    value: &mut u16,
    delta: i16,
    area: ratatui::layout::Rect,
    content_height: usize,
) -> bool {
    use jackin_tui::components::scrollable_panel::apply_scroll_delta;

    let before = *value;
    apply_scroll_delta(value, delta, scroll_viewport_height(area), content_height);
    *value != before
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

/// Center a dialog at a stable preferred width derived from `pct_w` of a 160-col
/// reference terminal. The dialog holds that width when the outer area is at least
/// that wide, and shrinks gracefully (min-margin 4) only when the terminal is too
/// narrow. This prevents dialogs from rescaling proportionally on every resize.
#[must_use]
pub fn centered_rect_fixed(
    outer: ratatui::layout::Rect,
    pct_w: u16,
    rows: u16,
) -> ratatui::layout::Rect {
    // Preferred = pct_w% of a 160-col reference — stable on any terminal ≥ that.
    const REFERENCE_COLS: u16 = 160;
    let preferred = REFERENCE_COLS.saturating_mul(pct_w) / 100;
    centered_rect_preferred(outer, preferred, rows)
}

/// Center a dialog at `preferred_w` columns, shrinking only when the outer area is
/// too narrow to fit `preferred_w` with a 4-column side margin.
#[must_use]
pub fn centered_rect_preferred(
    outer: ratatui::layout::Rect,
    preferred_w: u16,
    rows: u16,
) -> ratatui::layout::Rect {
    let w = preferred_w.min(outer.width.saturating_sub(4));
    let h = rows.min(outer.height);
    ratatui::layout::Rect {
        x: outer.x + outer.width.saturating_sub(w) / 2,
        y: outer.y + outer.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

pub mod list;

#[cfg(test)]
mod tests;
