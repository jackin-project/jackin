//! Launch docker-build log overlay helpers.

use jackin_tui::components::{viewport_height, viewport_width};
use jackin_tui::theme::{DIALOG_SURFACE, PHOSPHOR_DIM};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::LaunchView;

#[must_use]
pub fn build_log_scroll_filled(area: Rect) -> usize {
    let box_area = Rect {
        height: area.height.saturating_sub(1),
        ..area
    };
    let viewport_w = viewport_width(box_area);
    let viewport_h = viewport_height(box_area);
    let raw = crate::build_log::snapshot();
    let line_count = if raw.is_empty() {
        1
    } else {
        wrap_build_log_lines(raw, viewport_w).len()
    };
    jackin_tui::scroll::max_offset(line_count, viewport_h)
}

pub fn scroll_build_log(view: &mut LaunchView, area: Rect, delta: isize) {
    let filled = build_log_scroll_filled(area);
    view.build_log_scroll.scroll_by(filled, delta);
}

pub const BUILD_LOG_WRAP_PREFIX: &str = "↳ ";

#[must_use]
pub fn wrap_build_log_lines(raw: Vec<String>, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    raw.into_iter()
        .flat_map(|line| wrap_build_log_line(&line, width))
        .collect()
}

fn wrap_build_log_line(line: &str, width: usize) -> Vec<Line<'static>> {
    if line.is_empty() {
        return vec![Line::from(String::new())];
    }

    let default_style = Style::default().fg(Color::Gray).bg(DIALOG_SURFACE);
    let spans = jackin_tui::ansi_text::styled_spans(line.trim_end(), default_style);
    wrap_build_log_spans(spans, width)
}

fn wrap_build_log_spans(spans: Vec<Span<'static>>, width: usize) -> Vec<Line<'static>> {
    let mut cells: Vec<(char, Style)> = Vec::new();
    for span in spans {
        let style = span.style;
        cells.extend(span.content.chars().map(|ch| (ch, style)));
    }
    if cells.is_empty() {
        return vec![Line::from(String::new())];
    }

    let mut lines = Vec::new();
    let continuation_width = width
        .saturating_sub(BUILD_LOG_WRAP_PREFIX.chars().count())
        .max(1);
    let mut pos = 0;
    let mut first_line = true;
    while pos < cells.len() {
        let limit = if first_line {
            width
        } else {
            continuation_width
        };
        let hard_end = pos.saturating_add(limit).min(cells.len());
        let (line_end, mut next) = if hard_end < cells.len()
            && let Some(space) = (pos + 1..hard_end)
                .rev()
                .find(|idx| cells[*idx].0.is_whitespace())
        {
            (space, space + 1)
        } else {
            (hard_end, hard_end)
        };
        while next < cells.len() && cells[next].0.is_whitespace() {
            next += 1;
        }
        let line_cells = if line_end == pos {
            &cells[pos..hard_end]
        } else {
            &cells[pos..line_end]
        };
        push_wrapped_build_line(&mut lines, spans_from_cells(line_cells), first_line);
        first_line = false;
        pos = if line_end == pos { hard_end } else { next };
    }
    lines
}

fn spans_from_cells(cells: &[(char, Style)]) -> Vec<Span<'static>> {
    coalesce_cells(cells.iter().copied())
}

fn push_wrapped_build_line(
    lines: &mut Vec<Line<'static>>,
    mut spans: Vec<Span<'static>>,
    first_line: bool,
) {
    if !first_line {
        spans.insert(
            0,
            Span::styled(
                BUILD_LOG_WRAP_PREFIX,
                Style::default().fg(PHOSPHOR_DIM).bg(DIALOG_SURFACE),
            ),
        );
    }
    lines.push(Line::from(spans));
}

fn coalesce_cells<I>(cells: I) -> Vec<Span<'static>>
where
    I: IntoIterator<Item = (char, Style)>,
{
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (ch, style) in cells {
        if let Some(last) = spans.last_mut()
            && last.style == style
        {
            last.content.to_mut().push(ch);
            continue;
        }
        spans.push(Span::styled(ch.to_string(), style));
    }
    spans
}
