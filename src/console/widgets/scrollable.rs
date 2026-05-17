//! Shared scroll geometry and scrollbar rendering for console widgets.

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use super::{PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

pub(crate) const fn viewport_width(area: Rect) -> usize {
    area.width.saturating_sub(2) as usize
}

pub(crate) const fn viewport_height(area: Rect) -> usize {
    area.height.saturating_sub(2) as usize
}

pub(crate) const fn max_offset(content_len: usize, viewport: usize) -> u16 {
    if viewport == 0 || content_len <= viewport {
        0
    } else {
        let max = content_len.saturating_sub(viewport);
        // Silent truncation: an overflow greater than u16::MAX cannot be fully
        // addressed by a u16 scroll offset. Debug builds surface this.
        debug_assert!(
            max <= u16::MAX as usize,
            "scroll overflow (content_len - viewport) exceeds u16::MAX — scrollbar position truncated"
        );
        if max > u16::MAX as usize {
            u16::MAX
        } else {
            max as u16
        }
    }
}

pub(crate) const fn is_scrollable(content_len: usize, viewport: usize) -> bool {
    viewport > 0 && content_len > viewport
}

pub(crate) const fn effective_offset(content_len: usize, viewport: usize, offset: u16) -> u16 {
    let max = max_offset(content_len, viewport);
    if offset > max { max } else { offset }
}

pub(crate) fn scrollbar_position_for_offset(
    content_length: usize,
    viewport: usize,
    offset: usize,
) -> usize {
    if is_scrollable(content_length, viewport) {
        offset.min(content_length.saturating_sub(viewport))
    } else {
        0
    }
}

pub(crate) fn cursor_follow_offset(
    cursor: usize,
    content_length: usize,
    viewport: usize,
    stored_offset: u16,
) -> u16 {
    if viewport == 0 {
        return 0;
    }

    let max = max_offset(content_length, viewport);
    let stored = stored_offset.min(max);
    let raw = if cursor < usize::from(stored) {
        cursor.min(usize::from(u16::MAX)) as u16
    } else if is_scrollable(content_length, viewport)
        && cursor >= usize::from(stored).saturating_add(viewport)
    {
        cursor
            .saturating_add(1)
            .saturating_sub(viewport)
            .min(usize::from(u16::MAX)) as u16
    } else {
        stored
    };
    raw.min(max)
}

fn scrollbar_thumb_geometry(
    content_length: usize,
    viewport: usize,
    track_len: usize,
    offset: usize,
) -> (usize, usize) {
    if !is_scrollable(content_length, viewport) || track_len == 0 {
        return (0, 0);
    }

    let thumb_len = track_len
        .saturating_mul(viewport)
        .checked_div(content_length)
        .unwrap_or(0)
        .max(1)
        .min(track_len);
    let max_start = track_len.saturating_sub(thumb_len);
    let max_offset = content_length.saturating_sub(viewport);
    let offset = offset.min(max_offset);
    let thumb_start = offset
        .saturating_mul(max_start)
        .saturating_add(max_offset / 2)
        .checked_div(max_offset)
        .unwrap_or(0);

    (thumb_start, thumb_len)
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

    let (_, thumb_len) = scrollbar_thumb_geometry(content_length, viewport, track_len, 0);
    let max_thumb_start = track_len.saturating_sub(thumb_len);
    let max_scroll = content_length.saturating_sub(viewport);
    if max_thumb_start == 0 {
        return 0;
    }

    let thumb_start = track_position.min(max_thumb_start);
    let offset = thumb_start
        .saturating_mul(max_scroll)
        .saturating_add(max_thumb_start / 2)
        .checked_div(max_thumb_start)
        .unwrap_or(0);
    offset.min(usize::from(u16::MAX)) as u16
}

pub(crate) const fn apply_scroll_delta(value: &mut u16, delta: i16) {
    *value = if delta.is_negative() {
        value.saturating_sub(delta.unsigned_abs())
    } else {
        value.saturating_add(delta as u16)
    };
}

pub(crate) fn apply_horizontal_scroll_delta(
    value: &mut u16,
    delta: i16,
    viewport: usize,
    content_width: usize,
) {
    let max = max_offset(content_width, viewport);
    let current = (*value).min(max);
    let next = if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta as u16)
    };
    *value = next.min(max);
}

pub(crate) fn line_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum()
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
    let position = scrollbar_position_for_offset(content_width, viewport, usize::from(scroll_x));
    let area = horizontal_scrollbar_area(block_area);
    frame.render_widget(
        FixedScrollbar {
            content_length: content_width,
            viewport,
            position,
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
    let position = scrollbar_position_for_offset(content_height, viewport, usize::from(scroll_y));
    frame.render_widget(
        FixedScrollbar {
            content_length: content_height,
            viewport,
            position,
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
    let offset = usize::from(cursor_follow_offset(
        selected.unwrap_or(0),
        total,
        viewport,
        0,
    ));
    render_lines_with_offset_in_area(frame, area, lines, offset.min(usize::from(u16::MAX)) as u16);
}

pub(crate) fn render_lines_with_offset_in_area(
    frame: &mut Frame,
    area: Rect,
    lines: Vec<Line<'_>>,
    offset: u16,
) {
    let viewport = usize::from(area.height);
    let total = lines.len();
    let offset = usize::from(effective_offset(total, viewport, offset));
    let visible: Vec<Line<'_>> = lines.into_iter().skip(offset).take(viewport).collect();
    frame.render_widget(Paragraph::new(visible), area);
    if is_scrollable(total, viewport) {
        render_vertical_scrollbar_in_area(
            frame,
            vertical_list_scrollbar_area(area),
            total,
            viewport,
            offset.min(usize::from(u16::MAX)) as u16,
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

#[derive(Clone, Copy)]
enum FixedScrollbarOrientation {
    Horizontal,
    Vertical,
}

struct FixedScrollbar {
    content_length: usize,
    viewport: usize,
    position: usize,
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

        let (thumb_start, thumb_len) =
            scrollbar_thumb_geometry(self.content_length, self.viewport, track_len, self.position);
        let thumb_end = thumb_start.saturating_add(thumb_len);
        for idx in 0..track_len {
            let (x, y, symbol, style) = match self.orientation {
                FixedScrollbarOrientation::Horizontal => (
                    area.x.saturating_add(idx as u16),
                    area.y,
                    if (thumb_start..thumb_end).contains(&idx) {
                        "━"
                    } else {
                        "·"
                    },
                    if (thumb_start..thumb_end).contains(&idx) {
                        Style::default().fg(PHOSPHOR_DIM)
                    } else {
                        Style::default().fg(PHOSPHOR_DARK)
                    },
                ),
                FixedScrollbarOrientation::Vertical => (
                    area.x,
                    area.y.saturating_add(idx as u16),
                    if (thumb_start..thumb_end).contains(&idx) {
                        "█"
                    } else {
                        "·"
                    },
                    if (thumb_start..thumb_end).contains(&idx) {
                        Style::default().fg(PHOSPHOR_DIM)
                    } else {
                        Style::default().fg(PHOSPHOR_DARK)
                    },
                ),
            };
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
    let border_color = if focused {
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
    let content_width = max_line_width(&lines);
    let content_height = lines.len();
    let viewport_w = viewport_width(area);
    let viewport_h = viewport_height(area);
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
        cursor_follow_offset, render_vertical_scrollbar_in_area,
        scrollbar_offset_for_track_position, scrollbar_thumb_geometry,
    };
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    #[test]
    fn scrollbar_thumb_length_is_offset_invariant() {
        let lengths: Vec<usize> = (0..=2)
            .map(|offset| scrollbar_thumb_geometry(12, 10, 10, offset).1)
            .collect();

        assert_eq!(lengths, vec![8, 8, 8]);
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

        assert_eq!(rendered_thumb_len(0), 8);
        assert_eq!(rendered_thumb_len(1), 8);
        assert_eq!(rendered_thumb_len(2), 8);
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
    fn track_position_maps_to_scrollbar_thumb_range() {
        assert_eq!(scrollbar_offset_for_track_position(20, 5, 10, 0), 0);
        assert_eq!(scrollbar_offset_for_track_position(20, 5, 10, 9), 15);
    }
}
