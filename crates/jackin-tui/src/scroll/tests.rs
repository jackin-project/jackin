// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `scroll`.
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

#[test]
fn selectable_list_wheel_up_at_bottom_is_not_undone_by_cursor_follow() {
    let mut selected = 19;
    let mut offset = 15;

    assert!(scroll_selectable_list(
        &mut selected,
        &mut offset,
        20,
        5,
        -1,
    ));

    assert_eq!(offset, 14);
    assert_eq!(selected, 18);
    assert_eq!(
        cursor_follow_offset(selected, 20, 5, usize::from(offset)),
        usize::from(offset)
    );
}

#[test]
fn selectable_list_wheel_down_at_top_keeps_selection_visible() {
    let mut selected = 0;
    let mut offset = 0;

    assert!(scroll_selectable_list(&mut selected, &mut offset, 20, 5, 1));

    assert_eq!(offset, 1);
    assert_eq!(selected, 1);
    assert_eq!(
        cursor_follow_offset(selected, 20, 5, usize::from(offset)),
        usize::from(offset)
    );
}

#[test]
fn selectable_list_that_fits_keeps_selection_and_clears_offset() {
    let mut selected = 3;
    let mut offset = 2;

    assert!(!scroll_selectable_list(
        &mut selected,
        &mut offset,
        4,
        10,
        1
    ));

    assert_eq!(offset, 0);
    assert_eq!(selected, 3);
}

#[test]
fn mouse_scroll_delta_honors_visible_axes() {
    use crossterm::event::{KeyModifiers, MouseEventKind};

    assert_eq!(
        mouse_scroll_delta(
            MouseEventKind::ScrollDown,
            KeyModifiers::NONE,
            ScrollAxes {
                vertical: false,
                horizontal: true,
            },
        ),
        None
    );
    assert_eq!(
        mouse_scroll_delta(
            MouseEventKind::ScrollDown,
            KeyModifiers::SHIFT,
            ScrollAxes {
                vertical: true,
                horizontal: true,
            },
        ),
        Some(ScrollDelta {
            axis: ScrollAxis::Horizontal,
            amount: DEFAULT_HORIZONTAL_SCROLL_STEP as i16,
        })
    );
}
