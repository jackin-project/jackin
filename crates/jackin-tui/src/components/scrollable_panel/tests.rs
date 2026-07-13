// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `scrollable_panel`.
use super::{
    SCROLLBAR_HORIZONTAL_THUMB, ScrollbarStyle, apply_scroll_delta, apply_scroll_delta_unclamped,
    clamp_scroll_offset, cursor_follow_offset, render_horizontal_scrollbar,
    render_line_with_fixed_prefix_scroll, render_scrollable_block, render_scrollable_block_at,
    render_selected_lines_in_area, render_vertical_scrollbar_in_area,
    render_vertical_scrollbar_in_area_with_style, scrollbar_offset_for_track_position,
    scrollbar_thumb_geometry,
};
use crate::theme::{DIALOG_SCROLL_THUMB, DIALOG_SCROLL_TRACK, PHOSPHOR_GREEN};
use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Style, text::Line};

#[test]
fn scrollbar_thumb_length_is_offset_invariant() {
    let lengths: Vec<usize> = (0..=2)
        .map(|offset| scrollbar_thumb_geometry(12, 10, 10, offset).1)
        .collect();

    assert_eq!(lengths, vec![9, 9, 9]);
}

#[test]
fn vertical_scrollbar_thumb_moves_without_resizing() {
    fn rendered_thumb_len(scroll_y: u16) -> usize {
        let backend = TestBackend::new(1, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_vertical_scrollbar_in_area(frame, Rect::new(0, 0, 1, 10), 12, 10, scroll_y);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        (0..10).filter(|y| buffer[(0, *y)].symbol() == "┃").count()
    }

    assert_eq!(rendered_thumb_len(0), 9);
    assert_eq!(rendered_thumb_len(1), 9);
    assert_eq!(rendered_thumb_len(2), 9);
}

#[test]
fn scrollbar_uses_shared_dialog_scroll_palette() {
    let backend = TestBackend::new(1, 10);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render_vertical_scrollbar_in_area(frame, Rect::new(0, 0, 1, 10), 20, 5, 0);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), "┃");
    assert_eq!(buffer[(0, 0)].fg, DIALOG_SCROLL_THUMB);
    assert_eq!(buffer[(0, 9)].symbol(), "·");
    assert_eq!(buffer[(0, 9)].fg, DIALOG_SCROLL_TRACK);
}

#[test]
fn line_style_uses_matching_heavy_glyphs_per_axis() {
    // The default Line style reads identically across axes: a heavy horizontal
    // rule `━` and a heavy vertical rule `┃` (same weight).
    assert_eq!(SCROLLBAR_HORIZONTAL_THUMB, "━");
    assert_eq!(ScrollbarStyle::Line.vertical_thumb(), "┃");
}

#[test]
fn horizontal_thumb_is_always_the_heavy_line_regardless_of_vertical_style() {
    // Block weight is vertical-only; a horizontal bar always uses the heavy
    // line `━` (the full block reads poorly as a horizontal bar).
    let backend = TestBackend::new(12, 3);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_horizontal_scrollbar(frame, Rect::new(0, 0, 12, 3), 40, 0);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(
        (0..12).any(|x| (0..3).any(|y| buffer[(x, y)].symbol() == "━")),
        "horizontal scrollbar must paint the heavy line `━`"
    );
    assert!(
        (0..12).all(|x| (0..3).all(|y| buffer[(x, y)].symbol() != "█")),
        "horizontal scrollbar must never paint a full block"
    );
}

#[test]
fn vertical_block_style_renders_full_block_thumb() {
    let backend = TestBackend::new(1, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_vertical_scrollbar_in_area_with_style(
                frame,
                Rect::new(0, 0, 1, 10),
                20,
                5,
                0,
                ScrollbarStyle::Block,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), "█");
    assert_eq!(buffer[(0, 9)].symbol(), "·");
}

#[test]
fn cursor_follow_offset_keeps_cursor_in_view() {
    assert_eq!(cursor_follow_offset(0, 20, 5, 0), 0);
    assert_eq!(cursor_follow_offset(4, 20, 5, 0), 0);
    assert_eq!(cursor_follow_offset(5, 20, 5, 0), 1);
    assert_eq!(cursor_follow_offset(10, 20, 5, 0), 6);
    assert_eq!(cursor_follow_offset(19, 20, 5, 0), 15);
    assert_eq!(cursor_follow_offset(99, 20, 5, 0), 15);
    assert_eq!(cursor_follow_offset(7, 20, 0, 0), 0);
}

#[test]
fn clamp_scroll_offset_updates_stored_offset() {
    let mut scroll_x = 400;

    let effective = clamp_scroll_offset(100, 60, &mut scroll_x);

    assert_eq!(effective, 40);
    assert_eq!(scroll_x, 40);
}

#[test]
fn render_scrollable_block_at_clamps_without_mutating_offsets() {
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    let scroll_x = 400;
    let scroll_y = 400;
    let lines = (0..20)
        .map(|idx| Line::from(format!("row-{idx:02}-long-content")))
        .collect::<Vec<_>>();

    terminal
        .draw(|frame| {
            render_scrollable_block_at(
                frame,
                Rect::new(0, 0, 20, 5),
                lines,
                scroll_x,
                scroll_y,
                true,
                None,
            );
        })
        .unwrap();

    assert_eq!(scroll_x, 400);
    assert_eq!(scroll_y, 400);
}

#[test]
fn apply_scroll_delta_unclamped_moves_from_current_offset() {
    let mut scroll_x = 40;

    apply_scroll_delta_unclamped(&mut scroll_x, -8);

    assert_eq!(scroll_x, 32);
}

#[test]
fn fixed_prefix_scroll_keeps_prefix_and_background_visible() {
    let backend = TestBackend::new(8, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    let style = Style::default().bg(PHOSPHOR_GREEN);
    let line = Line::styled("▸  abcdef  ", style);

    terminal
        .draw(|frame| {
            render_line_with_fixed_prefix_scroll(frame, Rect::new(0, 0, 8, 1), 0, line, 3, 2);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), "▸");
    assert_eq!(buffer[(3, 0)].symbol(), "c");
    for x in 0..8 {
        assert_eq!(buffer[(x, 0)].bg, PHOSPHOR_GREEN, "x={x}");
    }
}

#[test]
fn fixed_prefix_scroll_fills_background_past_short_suffix() {
    let backend = TestBackend::new(8, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    let style = Style::default().bg(PHOSPHOR_GREEN);
    let line = Line::styled("▸  abc", style);

    terminal
        .draw(|frame| {
            render_line_with_fixed_prefix_scroll(frame, Rect::new(0, 0, 8, 1), 0, line, 3, 5);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), "▸");
    for x in 0..8 {
        assert_eq!(buffer[(x, 0)].bg, PHOSPHOR_GREEN, "x={x}");
    }
}

#[test]
fn fixed_prefix_scroll_uses_display_columns_for_wide_chars() {
    let backend = TestBackend::new(8, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    let style = Style::default().bg(PHOSPHOR_GREEN);
    let line = Line::styled("▸  a日本z", style);

    terminal
        .draw(|frame| {
            render_line_with_fixed_prefix_scroll(frame, Rect::new(0, 0, 8, 1), 0, line, 3, 1);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), "▸");
    assert_eq!(buffer[(3, 0)].symbol(), "日");
    assert_eq!(buffer[(5, 0)].symbol(), "本");
    assert_eq!(buffer[(7, 0)].symbol(), "z");
    for x in [0, 1, 2, 3, 5, 7] {
        assert_eq!(buffer[(x, 0)].bg, PHOSPHOR_GREEN, "x={x}");
    }
}

#[test]
fn track_position_maps_to_scrollbar_thumb_range() {
    assert_eq!(scrollbar_offset_for_track_position(20, 5, 10, 0), 0);
    assert_eq!(scrollbar_offset_for_track_position(20, 5, 10, 9), 15);
}

#[test]
fn track_position_does_not_snap_long_thumb_to_end() {
    assert_eq!(scrollbar_offset_for_track_position(12, 10, 10, 2), 0);
    assert_eq!(scrollbar_offset_for_track_position(12, 10, 10, 5), 1);
    assert_eq!(scrollbar_offset_for_track_position(12, 10, 10, 9), 2);
}

#[test]
fn scrollable_block_scrollbar_thumbs_reach_visible_ends() {
    let backend = TestBackend::new(12, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut scroll_x = 10;
    let mut scroll_y = 4;
    let lines: Vec<Line<'static>> = (0..8)
        .map(|idx| Line::from(format!("{idx:02}-abcdefghijklmnopq")))
        .collect();

    terminal
        .draw(|frame| {
            render_scrollable_block(
                frame,
                Rect::new(0, 0, 12, 6),
                lines,
                &mut scroll_x,
                &mut scroll_y,
                true,
                Some(" Test "),
            );
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(10, 5)].symbol(), "━");
    assert_eq!(buffer[(11, 4)].symbol(), "┃");
}

#[test]
fn scrollable_block_scrollbar_thumbs_are_proportional_to_viewport() {
    let backend = TestBackend::new(12, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut scroll_x = 0;
    let mut scroll_y = 0;
    let lines: Vec<Line<'static>> = (0..5)
        .map(|idx| Line::from(format!("{idx:02}-abcdefgh")))
        .collect();

    terminal
        .draw(|frame| {
            render_scrollable_block(
                frame,
                Rect::new(0, 0, 12, 6),
                lines,
                &mut scroll_x,
                &mut scroll_y,
                true,
                Some(" Test "),
            );
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let horizontal_thumb_len = (1..=10).filter(|x| buffer[(*x, 5)].symbol() == "━").count();
    let vertical_thumb_len = (1..=4).filter(|y| buffer[(11, *y)].symbol() == "┃").count();

    assert_eq!(horizontal_thumb_len, 9);
    assert_eq!(vertical_thumb_len, 3);
}

#[test]
fn scrollable_block_preserves_matching_right_padding_at_horizontal_end() {
    let backend = TestBackend::new(8, 4);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut scroll_x = 99;
    let mut scroll_y = 0;
    let lines = vec![Line::from("  abcdefgh")];

    terminal
        .draw(|frame| {
            render_scrollable_block(
                frame,
                Rect::new(0, 0, 8, 4),
                lines,
                &mut scroll_x,
                &mut scroll_y,
                true,
                Some(" Test "),
            );
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let visible: String = (1..=6).map(|x| buffer[(x, 1)].symbol()).collect();

    assert_eq!(scroll_x, 6);
    assert_eq!(visible, "efgh  ");
}

#[test]
fn scrollable_block_clamps_scroll_y_in_place() {
    let backend = TestBackend::new(12, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut scroll_x = 0;
    let mut scroll_y = 99;
    let lines: Vec<Line<'static>> = (0..8).map(|idx| Line::from(format!("{idx:02}"))).collect();

    terminal
        .draw(|frame| {
            render_scrollable_block(
                frame,
                Rect::new(0, 0, 12, 6),
                lines,
                &mut scroll_x,
                &mut scroll_y,
                false,
                None,
            );
        })
        .unwrap();

    assert_eq!(scroll_y, 4);
}

#[test]
fn apply_scroll_delta_clamps_at_max() {
    // content=12, viewport=5 → max=7. Start at 3, delta +10 → clamped to 7.
    let mut value: u16 = 3;
    apply_scroll_delta(&mut value, 10, 5, 12);
    assert_eq!(value, 7);
}

#[test]
fn apply_scroll_delta_corrects_overclamped_initial_value() {
    // value already above max; delta +1 should produce max, not max+1+stale_excess.
    let mut value: u16 = 20;
    apply_scroll_delta(&mut value, 1, 5, 12); // max=7, current=20.min(7)=7, 7+1=8>7 → 7
    assert_eq!(value, 7);
}

#[test]
fn apply_scroll_delta_saturates_at_zero() {
    let mut value: u16 = 0;
    apply_scroll_delta(&mut value, -5, 5, 12);
    assert_eq!(value, 0);
}

#[test]
fn scrollbar_thumb_geometry_returns_zero_for_empty_track() {
    assert_eq!(scrollbar_thumb_geometry(12, 10, 0, 0), (0, 0));
}

#[test]
fn scrollbar_thumb_geometry_returns_zero_when_not_scrollable() {
    assert_eq!(scrollbar_thumb_geometry(5, 10, 10, 0), (0, 0));
    assert_eq!(scrollbar_thumb_geometry(10, 10, 10, 0), (0, 0));
}

#[test]
fn scrollbar_thumb_geometry_single_overflow_row_stays_in_track() {
    // content=11, viewport=10, 1 overflow row. track=10.
    let (start_0, len_0) = scrollbar_thumb_geometry(11, 10, 10, 0);
    let (start_1, len_1) = scrollbar_thumb_geometry(11, 10, 10, 1);
    assert_eq!(len_0, len_1, "thumb length must be offset-invariant");
    assert_eq!(start_0, 0);
    assert!(start_1 > 0);
    assert_eq!(start_1 + len_1, 10, "thumb must reach track end");
}

#[test]
fn cursor_follow_offset_keeps_stored_when_cursor_in_view() {
    // stored=3, viewport=5: cursor rows 3..8 visible. cursor=6 is in range → keep stored.
    assert_eq!(cursor_follow_offset(6, 20, 5, 3), 3);
    // cursor=7 (last visible row) → also keep stored.
    assert_eq!(cursor_follow_offset(7, 20, 5, 3), 3);
}

#[test]
fn scrollbar_offset_for_track_position_midpoint() {
    // content=20, viewport=5, track=10 → max_scroll=15. Midpoint should land between 0 and 15.
    let mid = scrollbar_offset_for_track_position(20, 5, 10, 5);
    assert!(
        mid > 0 && mid < 15,
        "midpoint offset={mid} should be between 0 and 15"
    );
}

#[test]
fn render_selected_lines_in_area_shows_scrollbar_when_content_overflows() {
    let backend = TestBackend::new(10, 3);
    let mut terminal = Terminal::new(backend).unwrap();
    let lines: Vec<Line<'static>> = (0..5).map(|i| Line::from(format!("line {i}"))).collect();

    terminal
        .draw(|frame| {
            render_selected_lines_in_area(frame, Rect::new(0, 0, 10, 3), lines, Some(0));
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let has_scrollbar = (0..3).any(|y| ["┃", "·"].contains(&buffer[(9, y)].symbol()));
    assert!(
        has_scrollbar,
        "scrollbar expected when 5 lines overflow 3-row area"
    );
}

#[test]
fn render_selected_lines_in_area_highlights_full_width_when_content_fits() {
    let backend = TestBackend::new(10, 3);
    let mut terminal = Terminal::new(backend).unwrap();
    let lines = vec![Line::from("  abc"), Line::from("  def")];

    terminal
        .draw(|frame| {
            render_selected_lines_in_area(frame, Rect::new(0, 0, 10, 3), lines, Some(0));
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    for x in 0..10 {
        assert_eq!(buffer[(x, 0)].bg, PHOSPHOR_GREEN, "x={x}");
    }
}

#[test]
fn render_selected_lines_in_area_highlight_stops_before_scrollbar_gutter() {
    let backend = TestBackend::new(10, 3);
    let mut terminal = Terminal::new(backend).unwrap();
    let lines: Vec<Line<'static>> = (0..5).map(|i| Line::from(format!("line {i}"))).collect();

    terminal
        .draw(|frame| {
            render_selected_lines_in_area(frame, Rect::new(0, 0, 10, 3), lines, Some(0));
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    for x in 0..9 {
        assert_eq!(buffer[(x, 0)].bg, PHOSPHOR_GREEN, "x={x}");
    }
    assert_ne!(
        buffer[(9, 0)].bg,
        PHOSPHOR_GREEN,
        "selected row must not paint behind the scrollbar gutter"
    );
    assert!(
        ["┃", "·"].contains(&buffer[(9, 0)].symbol()),
        "scrollbar must own the gutter cell"
    );
}

#[test]
fn render_selected_lines_in_area_no_scrollbar_when_content_fits() {
    let backend = TestBackend::new(10, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    let lines: Vec<Line<'static>> = (0..3).map(|i| Line::from(format!("line {i}"))).collect();

    terminal
        .draw(|frame| {
            render_selected_lines_in_area(frame, Rect::new(0, 0, 10, 5), lines, Some(0));
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let has_scrollbar = (0..5).any(|y| ["┃", "·"].contains(&buffer[(9, y)].symbol()));
    assert!(
        !has_scrollbar,
        "no scrollbar expected when 3 lines fit in 5-row area"
    );
}

#[test]
fn scrollbar_thumb_reaches_track_end_at_max_offset() {
    // Pins the rounding-bias invariant: thumb must reach the last track cell at max offset.
    let content = 20;
    let viewport = 5;
    let track = 10;
    let max_offset = content - viewport;
    let (start, len) = scrollbar_thumb_geometry(content, viewport, track, max_offset);
    assert_eq!(
        start + len,
        track,
        "thumb must occupy up to the final track cell at max offset"
    );
}

#[test]
fn render_lines_with_offset_in_area_skips_lines_before_offset() {
    use super::render_lines_with_offset_in_area;

    let backend = TestBackend::new(6, 3);
    let mut terminal = Terminal::new(backend).unwrap();
    let lines: Vec<Line<'static>> = (0..5).map(|i| Line::from(format!("L{i}"))).collect();

    terminal
        .draw(|frame| {
            render_lines_with_offset_in_area(frame, Rect::new(0, 0, 6, 3), lines, 2);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    // Lines 2, 3, 4 should appear (offset=2 skips L0, L1).
    let row0: String = (0..2).map(|x| buffer[(x, 0)].symbol()).collect();
    let row1: String = (0..2).map(|x| buffer[(x, 1)].symbol()).collect();
    let row2: String = (0..2).map(|x| buffer[(x, 2)].symbol()).collect();
    assert_eq!(row0, "L2");
    assert_eq!(row1, "L3");
    assert_eq!(row2, "L4");
}
