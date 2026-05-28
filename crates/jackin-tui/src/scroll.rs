//! Shared scrollbar state, metrics, and offset adapters.
//!
//! `tui-scrollbar` owns proportional metrics and pointer interaction math.
//! jackin' owns rendering, so this module exposes small helpers that convert
//! those metrics into the full-cell thumbs and clamped offsets used by the
//! host console, launch progress overlay, and capsule renderer.

use tui_scrollbar::{SUBCELL, ScrollLengths, ScrollMetrics};

/// Full-cell thumb geometry for jackin-owned renderers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FullCellThumb {
    /// 0-based cell inside the track where the thumb starts.
    pub start: u16,
    /// Number of cells the thumb spans.
    pub len: u16,
}

/// Tail-relative scroll offset used by live surfaces.
///
/// Externally `0` means "live tail / newest content". Internally the helper
/// clamps through the same top-relative `tui-scrollbar` metrics used by normal
/// panels before converting back to the tail-relative representation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TailScroll {
    offset: usize,
}

impl TailScroll {
    #[must_use]
    pub const fn new(offset: usize) -> Self {
        Self { offset }
    }

    #[must_use]
    pub const fn offset(self) -> usize {
        self.offset
    }

    pub fn scroll_by(&mut self, filled: usize, delta: isize) -> usize {
        let current = self.offset.min(filled);
        self.offset = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize).min(filled)
        };
        self.offset
    }

    pub fn clamp(&mut self, filled: usize) -> usize {
        self.offset = self.offset.min(filled);
        self.offset
    }

    #[must_use]
    pub fn to_top_offset(self, content_len: usize, viewport_len: usize) -> usize {
        let max = max_offset(content_len, viewport_len);
        max.saturating_sub(self.offset.min(max))
    }
}

#[must_use]
pub const fn is_scrollable(content_len: usize, viewport_len: usize) -> bool {
    viewport_len > 0 && content_len > viewport_len
}

#[must_use]
pub const fn max_offset(content_len: usize, viewport_len: usize) -> usize {
    if viewport_len == 0 || content_len <= viewport_len {
        0
    } else {
        content_len - viewport_len
    }
}

fn metrics(
    content_len: usize,
    viewport_len: usize,
    offset: usize,
    track_cells: u16,
) -> ScrollMetrics {
    ScrollMetrics::new(
        ScrollLengths {
            content_len,
            viewport_len,
        },
        offset,
        track_cells,
    )
}

#[must_use]
fn offset_after_delta(
    content_len: usize,
    viewport_len: usize,
    offset: usize,
    delta: isize,
) -> usize {
    let current = offset.min(max_offset(content_len, viewport_len));
    if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current
            .saturating_add(delta as usize)
            .min(max_offset(content_len, viewport_len))
    }
}

pub fn apply_delta_u16(
    content_len: usize,
    viewport_len: usize,
    offset: &mut u16,
    delta: isize,
) -> u16 {
    let next = offset_after_delta(content_len, viewport_len, usize::from(*offset), delta)
        .min(usize::from(u16::MAX)) as u16;
    *offset = next;
    next
}

#[must_use]
pub const fn max_offset_u16(content_len: usize, viewport_len: usize) -> u16 {
    let max = max_offset(content_len, viewport_len);
    if max > u16::MAX as usize {
        u16::MAX
    } else {
        max as u16
    }
}

#[must_use]
pub const fn effective_offset_u16(content_len: usize, viewport_len: usize, offset: u16) -> u16 {
    let max = max_offset_u16(content_len, viewport_len);
    if offset > max { max } else { offset }
}

pub const fn clamp_offset_u16(content_len: usize, viewport_len: usize, offset: &mut u16) -> u16 {
    let effective = effective_offset_u16(content_len, viewport_len, *offset);
    *offset = effective;
    effective
}

/// No upper clamp: render paths that know viewport/content clamp later.
pub const fn apply_delta_unclamped_u16(offset: &mut u16, delta: i16) {
    *offset = if delta.is_negative() {
        offset.saturating_sub(delta.unsigned_abs())
    } else {
        offset.saturating_add(delta as u16)
    };
}

#[must_use]
pub fn offset_for_track_position(
    content_len: usize,
    viewport_len: usize,
    track_cells: u16,
    track_position: usize,
) -> usize {
    if !is_scrollable(content_len, viewport_len) || track_cells <= 1 {
        return 0;
    }

    let metrics = metrics(content_len, viewport_len, 0, track_cells);
    let position = track_position
        .min(usize::from(track_cells).saturating_sub(1))
        .saturating_mul(SUBCELL)
        .saturating_add(SUBCELL / 2);
    let thumb_start = position.saturating_sub(metrics.thumb_len() / 2);
    metrics.offset_for_thumb_start(thumb_start)
}

#[must_use]
pub fn offset_for_track_position_u16(
    content_len: usize,
    viewport_len: usize,
    track_cells: usize,
    track_position: usize,
) -> u16 {
    if !is_scrollable(content_len, viewport_len) || track_cells <= 1 {
        return 0;
    }

    offset_for_track_position(
        content_len,
        viewport_len,
        track_cells.min(usize::from(u16::MAX)) as u16,
        track_position,
    )
    .min(usize::from(u16::MAX)) as u16
}

#[must_use]
pub fn cursor_follow_offset(
    cursor: usize,
    content_len: usize,
    viewport_len: usize,
    stored_offset: usize,
) -> usize {
    if viewport_len == 0 {
        return 0;
    }

    let max = max_offset(content_len, viewport_len);
    let stored = stored_offset.min(max);
    let raw = if cursor < stored {
        cursor
    } else if is_scrollable(content_len, viewport_len)
        && cursor >= stored.saturating_add(viewport_len)
    {
        cursor.saturating_add(1).saturating_sub(viewport_len)
    } else {
        stored
    };
    raw.min(max)
}

#[must_use]
pub fn full_cell_thumb(
    content_len: usize,
    viewport_len: usize,
    track_cells: u16,
    offset: usize,
) -> Option<FullCellThumb> {
    if !is_scrollable(content_len, viewport_len) || track_cells == 0 {
        return None;
    }
    let max = max_offset(content_len, viewport_len);
    let metrics = metrics(content_len, viewport_len, offset, track_cells);
    let len = metrics
        .thumb_len()
        .saturating_add(SUBCELL - 1)
        .saturating_div(SUBCELL)
        .max(1)
        .min(usize::from(track_cells).saturating_sub(1).max(1));
    let max_start = usize::from(track_cells).saturating_sub(len);
    let rounded_start = metrics
        .thumb_start()
        .saturating_add(SUBCELL / 2)
        .saturating_div(SUBCELL)
        .min(max_start);
    let clamped_offset = offset.min(max);
    let start = if clamped_offset == 0 {
        0
    } else if clamped_offset == max {
        max_start
    } else {
        rounded_start
    };
    (len > 0).then_some(FullCellThumb {
        start: start as u16,
        len: len as u16,
    })
}

/// Full-cell vertical thumb for tail-relative scrollback surfaces.
#[must_use]
pub fn tail_vertical_thumb(
    track_rows: u16,
    filled: usize,
    tail_offset: usize,
) -> Option<FullCellThumb> {
    if track_rows == 0 || filled == 0 {
        return None;
    }
    let content_len = filled.saturating_add(usize::from(track_rows));
    let viewport_len = usize::from(track_rows);
    let top_offset = TailScroll::new(tail_offset).to_top_offset(content_len, viewport_len);
    full_cell_thumb(content_len, viewport_len, track_rows, top_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_viewport_is_not_scrollable() {
        assert!(!is_scrollable(10, 0));
        assert_eq!(max_offset(10, 0), 0);
    }

    #[test]
    fn content_that_fits_has_zero_max_offset() {
        assert!(!is_scrollable(5, 5));
        assert_eq!(max_offset(5, 5), 0);
    }

    #[test]
    fn one_row_overflow_has_one_row_range() {
        assert_eq!(max_offset(11, 10), 1);
    }

    #[test]
    fn delta_starts_from_clamped_offset() {
        assert_eq!(offset_after_delta(12, 5, 99, -1), 6);
        assert_eq!(offset_after_delta(12, 5, 99, 1), 7);
    }

    #[test]
    fn u16_offset_helpers_clamp_and_move() {
        assert_eq!(max_offset_u16(12, 5), 7);
        assert_eq!(effective_offset_u16(12, 5, 99), 7);
        let mut clamped = 99;
        assert_eq!(clamp_offset_u16(12, 5, &mut clamped), 7);
        assert_eq!(clamped, 7);

        let mut loose = 40;
        apply_delta_unclamped_u16(&mut loose, -8);
        assert_eq!(loose, 32);
        apply_delta_unclamped_u16(&mut loose, 10);
        assert_eq!(loose, 42);
    }

    #[test]
    fn full_cell_thumb_reaches_track_end_at_max_offset() {
        let thumb = full_cell_thumb(20, 5, 10, 15).expect("overflowing content");
        assert_eq!(thumb.start + thumb.len, 10);
    }

    #[test]
    fn one_row_overflow_thumb_reaches_track_end_at_max_offset() {
        let thumb = full_cell_thumb(7, 6, 6, 1).expect("overflowing content");
        assert!(
            thumb.len < 6,
            "scrollable thumb must leave visible travel room"
        );
        assert_eq!(thumb.start + thumb.len, 6);
    }

    #[test]
    fn one_cell_track_drag_maps_to_zero_without_panicking() {
        assert_eq!(offset_for_track_position(7, 6, 1, 0), 0);
        assert_eq!(offset_for_track_position_u16(7, 6, 1, 0), 0);
    }

    #[test]
    fn tail_thumb_reaches_track_end_at_live_tail() {
        let thumb = tail_vertical_thumb(6, 1, 0).expect("overflowing content");
        assert_eq!(thumb.start + thumb.len, 6);
    }

    #[test]
    fn full_cell_thumb_moves_on_midpoint_drag_mapping() {
        let mid = offset_for_track_position(20, 5, 10, 5);
        assert!(mid > 0 && mid < 15);
        assert_eq!(offset_for_track_position_u16(20, 5, 10, 5), mid as u16);
    }

    #[test]
    fn tail_scroll_converts_to_top_offset() {
        let tail = TailScroll::new(0);
        assert_eq!(tail.to_top_offset(20, 5), 15);
    }

    #[test]
    fn tail_scroll_down_from_overshoot_moves_visible_content() {
        let mut tail = TailScroll::new(99);
        tail.scroll_by(15, -3);
        assert_eq!(tail.offset(), 12);
    }

    #[test]
    fn cursor_follow_keeps_selection_visible() {
        assert_eq!(cursor_follow_offset(0, 20, 5, 0), 0);
        assert_eq!(cursor_follow_offset(5, 20, 5, 0), 1);
        assert_eq!(cursor_follow_offset(19, 20, 5, 0), 15);
        assert_eq!(cursor_follow_offset(7, 20, 0, 0), 0);
    }
}
