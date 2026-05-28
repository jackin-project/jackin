//! Shared scroll geometry and scrollbar rendering for console widgets.

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Widget},
};

use super::{DIALOG_SCROLL_THUMB, DIALOG_SCROLL_TRACK, PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE};
use jackin_tui::scroll;

pub(crate) const fn viewport_width(area: Rect) -> usize {
    area.width.saturating_sub(2) as usize
}

pub(crate) const fn viewport_height(area: Rect) -> usize {
    area.height.saturating_sub(2) as usize
}

pub(crate) const fn max_offset(content_len: usize, viewport: usize) -> u16 {
    let max = scroll::max_offset(content_len, viewport);
    if max > u16::MAX as usize {
        u16::MAX
    } else {
        max as u16
    }
}

pub(crate) const fn is_scrollable(content_len: usize, viewport: usize) -> bool {
    scroll::is_scrollable(content_len, viewport)
}

pub(crate) const fn effective_offset(content_len: usize, viewport: usize, offset: u16) -> u16 {
    let max = max_offset(content_len, viewport);
    if offset > max { max } else { offset }
}

pub(crate) const fn clamp_scroll_offset(
    content_len: usize,
    viewport: usize,
    offset: &mut u16,
) -> u16 {
    let effective = effective_offset(content_len, viewport, *offset);
    *offset = effective;
    effective
}

pub(crate) fn cursor_follow_offset(
    cursor: usize,
    content_length: usize,
    viewport: usize,
    stored_offset: u16,
) -> u16 {
    scroll::cursor_follow_offset(cursor, content_length, viewport, usize::from(stored_offset))
        .min(usize::from(u16::MAX)) as u16
}

fn scrollbar_thumb_geometry(
    content_length: usize,
    viewport: usize,
    track_len: usize,
    offset: usize,
) -> (usize, usize) {
    scroll::full_cell_thumb(
        content_length,
        viewport,
        track_len.min(usize::from(u16::MAX)) as u16,
        offset,
    )
    .map_or((0, 0), |thumb| {
        (usize::from(thumb.start), usize::from(thumb.len))
    })
}

pub(crate) fn scrollbar_offset_for_track_position(
    content_length: usize,
    viewport: usize,
    track_len: usize,
    track_position: usize,
) -> u16 {
    if !is_scrollable(content_length, viewport) || track_len == 0 {
        return 0;
    }

    scroll::offset_for_track_position(
        content_length,
        viewport,
        track_len.min(usize::from(u16::MAX)) as u16,
        track_position,
    )
    .min(usize::from(u16::MAX)) as u16
}

// No upper clamp: every caller's render path calls effective_offset, which clamps.
pub(crate) const fn apply_scroll_delta_unclamped(value: &mut u16, delta: i16) {
    *value = if delta.is_negative() {
        value.saturating_sub(delta.unsigned_abs())
    } else {
        value.saturating_add(delta as u16)
    };
}

pub(crate) fn apply_scroll_delta(value: &mut u16, delta: i16, viewport: usize, content_len: usize) {
    scroll::apply_delta_u16(content_len, viewport, value, isize::from(delta));
}

pub(crate) fn apply_term_width_scroll_delta(
    value: &mut u16,
    delta: i16,
    term_width: u16,
    content_width: usize,
) {
    apply_scroll_delta(
        value,
        delta,
        usize::from(term_width.saturating_sub(2)),
        content_width,
    );
}

pub(crate) fn line_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| jackin_tui::display_cols(&span.content))
        .sum()
}

pub(crate) fn render_line_with_fixed_prefix_scroll(
    frame: &mut Frame,
    area: Rect,
    row: u16,
    line: Line<'static>,
    fixed_prefix_cols: usize,
    scroll_x: usize,
) {
    let mut fill_style = line.style;
    let mut styled_spans = Vec::new();
    let mut base_col = 0usize;
    for span in line.spans {
        let style = line.style.patch(span.style);
        if fill_style.bg.is_none() && style.bg.is_some() {
            fill_style = style;
        }
        let span_width = jackin_tui::display_cols(&span.content);
        styled_spans.push((span.content.into_owned(), style, base_col));
        base_col += span_width;
    }

    let width = usize::from(area.width);
    for col in 0..width {
        frame
            .buffer_mut()
            .set_string(area.x + col as u16, area.y + row, " ", fill_style);
    }

    for (text, style, base_col) in styled_spans {
        for segment in jackin_tui::fixed_prefix_scroll_segments(
            &text,
            base_col,
            fixed_prefix_cols,
            scroll_x,
            width,
        ) {
            frame.buffer_mut().set_string(
                area.x + segment.target_col as u16,
                area.y + row,
                &text[segment.start_byte..segment.end_byte],
                style,
            );
            for col in segment.target_col..segment.target_col + segment.display_cols {
                frame.buffer_mut()[(area.x + col as u16, area.y + row)].set_style(style);
            }
        }
    }
}

// Trailing padding mirrors leading spaces so indented content scrolls
// symmetrically — without it the rightmost indent column is unreachable.
fn leading_space_count(line: &Line<'_>) -> usize {
    let mut count = 0;
    for span in &line.spans {
        for ch in span.content.chars() {
            if ch != ' ' {
                return count;
            }
            count += 1;
        }
    }
    count
}

pub(crate) fn max_line_width(lines: &[Line<'_>]) -> usize {
    // Adds leading_space_count a second time to account for the matching trailing
    // padding that add_trailing_padding appends; the padded line is genuinely that
    // wide, so content_width must reflect it to keep the scrollbar range correct.
    lines
        .iter()
        .map(|l| line_width(l).saturating_add(leading_space_count(l)))
        .max()
        .unwrap_or(0)
}

fn add_trailing_padding(mut lines: Vec<Line<'_>>) -> Vec<Line<'_>> {
    for line in &mut lines {
        let padding = leading_space_count(line);
        if padding > 0 {
            line.spans.push(Span::raw(" ".repeat(padding)));
        }
    }
    lines
}

pub(crate) const fn horizontal_scrollbar_area(block_area: Rect) -> Rect {
    Rect {
        x: block_area.x + 1,
        y: block_area.y + block_area.height.saturating_sub(1),
        width: block_area.width.saturating_sub(2),
        height: 1,
    }
}

pub(crate) const fn vertical_scrollbar_area(block_area: Rect) -> Rect {
    Rect {
        x: block_area.x + block_area.width.saturating_sub(1),
        y: block_area.y + 1,
        width: 1,
        height: block_area.height.saturating_sub(2),
    }
}

pub(crate) fn render_horizontal_scrollbar(
    frame: &mut Frame,
    block_area: Rect,
    content_width: usize,
    scroll_x: u16,
) {
    let viewport = viewport_width(block_area);
    if !is_scrollable(content_width, viewport) {
        return;
    }
    let area = horizontal_scrollbar_area(block_area);
    frame.render_widget(
        FixedScrollbar {
            content_length: content_width,
            viewport,
            offset: scroll_x,
            orientation: FixedScrollbarOrientation::Horizontal,
        },
        area,
    );
}

pub(crate) fn render_vertical_scrollbar(
    frame: &mut Frame,
    block_area: Rect,
    content_height: usize,
    scroll_y: u16,
) {
    let viewport = viewport_height(block_area);
    if !is_scrollable(content_height, viewport) {
        return;
    }
    let area = vertical_scrollbar_area(block_area);
    render_vertical_scrollbar_in_area(frame, area, content_height, viewport, scroll_y);
}

pub(crate) fn render_vertical_scrollbar_in_area(
    frame: &mut Frame,
    area: Rect,
    content_height: usize,
    viewport: usize,
    scroll_y: u16,
) {
    if !is_scrollable(content_height, viewport) || area.height == 0 {
        return;
    }
    frame.render_widget(
        FixedScrollbar {
            content_length: content_height,
            viewport,
            offset: scroll_y,
            orientation: FixedScrollbarOrientation::Vertical,
        },
        area,
    );
}

pub(crate) fn render_selected_lines_in_area(
    frame: &mut Frame,
    area: Rect,
    lines: Vec<Line<'_>>,
    selected: Option<usize>,
) {
    let viewport = usize::from(area.height);
    let total = lines.len();
    let offset = cursor_follow_offset(selected.unwrap_or(0), total, viewport, 0);
    render_lines_with_offset_in_area(frame, area, lines, offset);
}

pub(crate) fn render_lines_with_offset_in_area(
    frame: &mut Frame,
    area: Rect,
    lines: Vec<Line<'_>>,
    offset: u16,
) {
    let viewport = usize::from(area.height);
    let total = lines.len();
    let clamped = effective_offset(total, viewport, offset);
    let visible: Text<'_> = lines
        .into_iter()
        .skip(usize::from(clamped))
        .take(viewport)
        .collect();
    frame.render_widget(Paragraph::new(visible), area);
    if is_scrollable(total, viewport) {
        render_vertical_scrollbar_in_area(
            frame,
            vertical_list_scrollbar_area(area),
            total,
            viewport,
            clamped,
        );
    }
}

const fn vertical_list_scrollbar_area(area: Rect) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(1),
        y: area.y,
        width: 1,
        height: area.height,
    }
}

#[derive(Clone, Copy, Debug)]
enum FixedScrollbarOrientation {
    Horizontal,
    Vertical,
}

#[derive(Debug)]
struct FixedScrollbar {
    content_length: usize,
    viewport: usize,
    offset: u16,
    orientation: FixedScrollbarOrientation,
}

impl Widget for FixedScrollbar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let track_len = match self.orientation {
            FixedScrollbarOrientation::Horizontal => usize::from(area.width),
            FixedScrollbarOrientation::Vertical => usize::from(area.height),
        };
        if track_len == 0 {
            return;
        }

        let (thumb_start, thumb_len) = scrollbar_thumb_geometry(
            self.content_length,
            self.viewport,
            track_len,
            usize::from(self.offset),
        );
        let thumb_end = thumb_start.saturating_add(thumb_len);
        // Hoist orientation constants out of the per-cell loop.
        let (thumb_sym, track_sym, base_x, base_y, dx, dy): (&str, &str, u16, u16, u16, u16) =
            match self.orientation {
                FixedScrollbarOrientation::Horizontal => ("━", "·", area.x, area.y, 1, 0),
                FixedScrollbarOrientation::Vertical => ("█", "·", area.x, area.y, 0, 1),
            };
        let thumb_style = Style::default().fg(DIALOG_SCROLL_THUMB);
        let track_style = Style::default().fg(DIALOG_SCROLL_TRACK);
        for idx in 0..track_len {
            let in_thumb = (thumb_start..thumb_end).contains(&idx);
            let i = idx as u16;
            let x = base_x.saturating_add(i * dx);
            let y = base_y.saturating_add(i * dy);
            let symbol = if in_thumb { thumb_sym } else { track_sym };
            let style = if in_thumb { thumb_style } else { track_style };
            buf.set_string(x, y, symbol, style);
        }
    }
}

pub(crate) fn render_scrollable_block(
    frame: &mut Frame,
    area: Rect,
    lines: Vec<Line<'_>>,
    scroll_x: &mut u16,
    scroll_y: &mut u16,
    focused: bool,
    title: Option<&str>,
) {
    let content_width = max_line_width(&lines);
    let content_height = lines.len();
    let viewport_w = viewport_width(area);
    let viewport_h = viewport_height(area);
    // Green border signals "you can scroll here". A focused but non-scrollable block
    // uses the default border so it doesn't imply scroll capability it doesn't have.
    let has_scroll =
        is_scrollable(content_width, viewport_w) || is_scrollable(content_height, viewport_h);
    let border_color = if focused && has_scroll {
        PHOSPHOR_GREEN
    } else {
        PHOSPHOR_DARK
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    if let Some(t) = title {
        block = block.title(Span::styled(
            t,
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    }
    let eff_x = effective_offset(content_width, viewport_w, *scroll_x);
    let eff_y = effective_offset(content_height, viewport_h, *scroll_y);
    *scroll_x = eff_x;
    *scroll_y = eff_y;
    frame.render_widget(
        Paragraph::new(add_trailing_padding(lines))
            .block(block)
            .style(Style::default().fg(PHOSPHOR_GREEN))
            .scroll((eff_y, eff_x)),
        area,
    );
    render_horizontal_scrollbar(frame, area, content_width, eff_x);
    render_vertical_scrollbar(frame, area, content_height, eff_y);
}

#[cfg(test)]
mod tests {
    use super::{
        apply_scroll_delta, apply_scroll_delta_unclamped, clamp_scroll_offset,
        cursor_follow_offset, render_line_with_fixed_prefix_scroll, render_scrollable_block,
        render_selected_lines_in_area, render_vertical_scrollbar_in_area,
        scrollbar_offset_for_track_position, scrollbar_thumb_geometry,
    };
    use crate::console::widgets::{DIALOG_SCROLL_THUMB, DIALOG_SCROLL_TRACK, PHOSPHOR_GREEN};
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
                    render_vertical_scrollbar_in_area(
                        frame,
                        Rect::new(0, 0, 1, 10),
                        12,
                        10,
                        scroll_y,
                    );
                })
                .unwrap();

            let buffer = terminal.backend().buffer();
            (0..10).filter(|y| buffer[(0, *y)].symbol() == "█").count()
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
        assert_eq!(buffer[(0, 0)].symbol(), "█");
        assert_eq!(buffer[(0, 0)].fg, DIALOG_SCROLL_THUMB);
        assert_eq!(buffer[(0, 9)].symbol(), "·");
        assert_eq!(buffer[(0, 9)].fg, DIALOG_SCROLL_TRACK);
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
        assert_eq!(buffer[(11, 4)].symbol(), "█");
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
        let vertical_thumb_len = (1..=4).filter(|y| buffer[(11, *y)].symbol() == "█").count();

        assert_eq!(horizontal_thumb_len, 9);
        assert_eq!(vertical_thumb_len, 4);
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
        let has_scrollbar = (0..3).any(|y| ["█", "·"].contains(&buffer[(9, y)].symbol()));
        assert!(
            has_scrollbar,
            "scrollbar expected when 5 lines overflow 3-row area"
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
        let has_scrollbar = (0..5).any(|y| ["█", "·"].contains(&buffer[(9, y)].symbol()));
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
}
