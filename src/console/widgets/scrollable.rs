//! Shared scroll geometry and scrollbar rendering for console widgets.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
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

const fn scrollbar_content_length(content_length: usize, viewport: usize) -> usize {
    // Ratatui sizes the thumb as viewport / scrollbar_content_length. Passing raw
    // content_length produces a thumb that is too small for moderate overflows.
    // The correct value is the number of distinct scroll positions: overflow + 1.
    if is_scrollable(content_length, viewport) {
        content_length.saturating_sub(viewport).saturating_add(1)
    } else {
        0
    }
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
    let mut state = ScrollbarState::new(scrollbar_content_length(content_width, viewport))
        .position(position)
        .viewport_content_length(viewport);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("·"))
        .thumb_symbol("━")
        .track_style(Style::default().fg(PHOSPHOR_DARK))
        .thumb_style(Style::default().fg(PHOSPHOR_DIM));
    let area = Rect {
        x: block_area.x + 1,
        y: block_area.y + block_area.height.saturating_sub(1),
        width: block_area.width.saturating_sub(2),
        height: 1,
    };
    frame.render_stateful_widget(scrollbar, area, &mut state);
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
    let area = Rect {
        x: block_area.x + block_area.width.saturating_sub(1),
        y: block_area.y + 1,
        width: 1,
        height: block_area.height.saturating_sub(2),
    };
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
    let mut state = ScrollbarState::new(scrollbar_content_length(content_height, viewport))
        .position(position)
        .viewport_content_length(viewport);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("·"))
        .thumb_symbol("█")
        .track_style(Style::default().fg(PHOSPHOR_DARK))
        .thumb_style(Style::default().fg(PHOSPHOR_DIM));
    frame.render_stateful_widget(scrollbar, area, &mut state);
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
