//! Shared scroll geometry, panels, and scrollbar rendering.

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{HighlightSpacing, List, ListItem, ListState, Paragraph, StatefulWidget, Widget},
};

use crate::{
    display_cols, fixed_prefix_scroll_segments, leading_space_cols, padded_line_display_cols,
    scroll,
    theme::{DIALOG_SCROLL_THUMB, DIALOG_SCROLL_TRACK},
};

use super::{Panel, PanelFocus};

/// Dim track glyph shared by every scrollbar, both orientations and styles.
pub const SCROLLBAR_TRACK: &str = "·";

/// Horizontal scrollbar thumb glyph. Style-independent: the heavy line is the
/// only horizontal thumb. A full block reads poorly as a horizontal bar, so
/// [`ScrollbarStyle`] applies to the vertical thumb only.
pub const SCROLLBAR_HORIZONTAL_THUMB: &str = "━";

/// Visual weight of the **vertical** scrollbar thumb. The track is always
/// [`SCROLLBAR_TRACK`] and the horizontal thumb is always
/// [`SCROLLBAR_HORIZONTAL_THUMB`]; this enum only chooses the vertical glyph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollbarStyle {
    /// Heavy box-drawing line `┃` — a thin centre rule matching the horizontal
    /// `━`. The default everywhere.
    #[default]
    Line,
    /// Full block `█` — a solid, heavy vertical bar.
    Block,
}

impl ScrollbarStyle {
    /// Vertical thumb glyph: heavy line `┃` (Line) or full block `█` (Block).
    #[must_use]
    pub const fn vertical_thumb(self) -> &'static str {
        match self {
            Self::Line => "┃",
            Self::Block => "█",
        }
    }
}

pub const fn viewport_width(area: Rect) -> usize {
    area.width.saturating_sub(2) as usize
}

pub const fn viewport_height(area: Rect) -> usize {
    area.height.saturating_sub(2) as usize
}

pub const fn max_offset(content_len: usize, viewport: usize) -> u16 {
    scroll::max_offset_u16(content_len, viewport)
}

pub const fn is_scrollable(content_len: usize, viewport: usize) -> bool {
    scroll::is_scrollable(content_len, viewport)
}

pub const fn effective_offset(content_len: usize, viewport: usize, offset: u16) -> u16 {
    scroll::effective_offset_u16(content_len, viewport, offset)
}

pub const fn clamp_scroll_offset(content_len: usize, viewport: usize, offset: &mut u16) -> u16 {
    scroll::clamp_offset_u16(content_len, viewport, offset)
}

pub fn cursor_follow_offset(
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

pub fn scrollbar_offset_for_track_position(
    content_length: usize,
    viewport: usize,
    track_len: usize,
    track_position: usize,
) -> u16 {
    scroll::offset_for_track_position_u16(content_length, viewport, track_len, track_position)
}

// No upper clamp: every caller's render path calls effective_offset, which clamps.
pub const fn apply_scroll_delta_unclamped(value: &mut u16, delta: i16) {
    scroll::apply_delta_unclamped_u16(value, delta);
}

pub fn apply_scroll_delta(value: &mut u16, delta: i16, viewport: usize, content_len: usize) {
    scroll::apply_delta_u16(content_len, viewport, value, isize::from(delta));
}

pub fn apply_term_width_scroll_delta(
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

pub fn line_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| display_cols(&span.content))
        .sum()
}

pub fn render_line_with_fixed_prefix_scroll(
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
        let span_width = display_cols(&span.content);
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
        for segment in
            fixed_prefix_scroll_segments(&text, base_col, fixed_prefix_cols, scroll_x, width)
        {
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
    leading_space_cols(line.spans.iter().map(|span| span.content.as_ref()))
}

pub fn max_line_width(lines: &[Line<'_>]) -> usize {
    // Adds leading_space_count a second time to account for the matching trailing
    // padding that add_trailing_padding appends; the padded line is genuinely that
    // wide, so content_width must reflect it to keep the scrollbar range correct.
    lines
        .iter()
        .map(|line| padded_line_display_cols(line.spans.iter().map(|span| span.content.as_ref())))
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

pub const fn horizontal_scrollbar_area(block_area: Rect) -> Rect {
    Rect {
        x: block_area.x + 1,
        y: block_area.y + block_area.height.saturating_sub(1),
        width: block_area.width.saturating_sub(2),
        height: 1,
    }
}

pub const fn vertical_scrollbar_area(block_area: Rect) -> Rect {
    Rect {
        x: block_area.x + block_area.width.saturating_sub(1),
        y: block_area.y + 1,
        width: 1,
        height: block_area.height.saturating_sub(2),
    }
}

/// Horizontal scrollbars have no style variant — the thumb is always
/// [`SCROLLBAR_HORIZONTAL_THUMB`] (the full block reads poorly horizontally).
pub fn render_horizontal_scrollbar(
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
            // Ignored for horizontal; the glyph is always the heavy line.
            style: ScrollbarStyle::Line,
        },
        area,
    );
}

pub fn render_vertical_scrollbar(
    frame: &mut Frame,
    block_area: Rect,
    content_height: usize,
    scroll_y: u16,
) {
    render_vertical_scrollbar_with_style(
        frame,
        block_area,
        content_height,
        scroll_y,
        ScrollbarStyle::Line,
    );
}

pub fn render_vertical_scrollbar_with_style(
    frame: &mut Frame,
    block_area: Rect,
    content_height: usize,
    scroll_y: u16,
    style: ScrollbarStyle,
) {
    let viewport = viewport_height(block_area);
    if !is_scrollable(content_height, viewport) {
        return;
    }
    let area = vertical_scrollbar_area(block_area);
    render_vertical_scrollbar_in_area_with_style(
        frame,
        area,
        content_height,
        viewport,
        scroll_y,
        style,
    );
}

pub fn render_vertical_scrollbar_in_area(
    frame: &mut Frame,
    area: Rect,
    content_height: usize,
    viewport: usize,
    scroll_y: u16,
) {
    render_vertical_scrollbar_in_area_with_style(
        frame,
        area,
        content_height,
        viewport,
        scroll_y,
        ScrollbarStyle::Line,
    );
}

pub fn render_vertical_scrollbar_in_area_with_style(
    frame: &mut Frame,
    area: Rect,
    content_height: usize,
    viewport: usize,
    scroll_y: u16,
    style: ScrollbarStyle,
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
            style,
        },
        area,
    );
}

pub fn render_selected_lines_in_area(
    frame: &mut Frame,
    area: Rect,
    lines: Vec<Line<'_>>,
    selected: Option<usize>,
) {
    let viewport = usize::from(area.height);
    let total = lines.len();
    let offset = cursor_follow_offset(selected.unwrap_or(0), total, viewport, 0);

    if selected.is_some() {
        // Use List so the selected row gets a full-width background fill — same
        // pattern as SelectList and the workspace sidebar.
        let highlight = Style::default()
            .bg(crate::theme::PHOSPHOR_GREEN)
            .fg(crate::theme::PHOSPHOR_DARK)
            .add_modifier(Modifier::BOLD);
        let items: Vec<ListItem<'_>> = lines.into_iter().map(ListItem::new).collect();
        let mut state = ListState::default()
            .with_offset(offset as usize)
            .with_selected(selected);
        let list = List::new(items)
            .highlight_style(highlight)
            .highlight_spacing(HighlightSpacing::Never);
        StatefulWidget::render(list, area, frame.buffer_mut(), &mut state);
        if is_scrollable(total, viewport) {
            render_vertical_scrollbar_in_area(
                frame,
                vertical_list_scrollbar_area(area),
                total,
                viewport,
                offset,
            );
        }
    } else {
        render_lines_with_offset_in_area(frame, area, lines, offset);
    }
}

pub fn render_lines_with_offset_in_area(
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
    style: ScrollbarStyle,
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
        // Hoist orientation constants out of the per-cell loop. The thumb glyph
        // is the only axis-dependent value; track is the shared dim dot.
        let (thumb_sym, base_x, base_y, dx, dy): (&str, u16, u16, u16, u16) = match self.orientation
        {
            FixedScrollbarOrientation::Horizontal => {
                (SCROLLBAR_HORIZONTAL_THUMB, area.x, area.y, 1, 0)
            }
            FixedScrollbarOrientation::Vertical => {
                (self.style.vertical_thumb(), area.x, area.y, 0, 1)
            }
        };
        let track_sym = SCROLLBAR_TRACK;
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

pub fn render_scrollable_block(
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
    let eff_x = effective_offset(content_width, viewport_w, *scroll_x);
    let eff_y = effective_offset(content_height, viewport_h, *scroll_y);
    *scroll_x = eff_x;
    *scroll_y = eff_y;
    render_scrollable_block_at(frame, area, lines, eff_x, eff_y, focused, title);
}

pub fn render_scrollable_block_at(
    frame: &mut Frame,
    area: Rect,
    lines: Vec<Line<'_>>,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
    title: Option<&str>,
) {
    let content_width = max_line_width(&lines);
    let content_height = lines.len();
    let viewport_w = viewport_width(area);
    let viewport_h = viewport_height(area);
    // All focused blocks get PHOSPHOR_GREEN border (WCAG focus-visible rule).
    // FocusedScrollable vs Focused is kept so callers can distinguish scroll
    // affordance, but both render green — the difference is informational only.
    let has_scroll =
        is_scrollable(content_width, viewport_w) || is_scrollable(content_height, viewport_h);
    let focus = if focused && has_scroll {
        PanelFocus::FocusedScrollable
    } else if focused {
        PanelFocus::Focused
    } else {
        PanelFocus::Unfocused
    };
    let mut panel = Panel::new().focus(focus);
    if let Some(title) = title {
        panel = panel.title(title);
    }
    let eff_x = effective_offset(content_width, viewport_w, scroll_x);
    let eff_y = effective_offset(content_height, viewport_h, scroll_y);
    frame.render_widget(
        Paragraph::new(add_trailing_padding(lines))
            .block(panel.block())
            .style(crate::theme::GREEN)
            .scroll((eff_y, eff_x)),
        area,
    );
    render_horizontal_scrollbar(frame, area, content_width, eff_x);
    render_vertical_scrollbar(frame, area, content_height, eff_y);
}

#[cfg(test)]
mod tests;
