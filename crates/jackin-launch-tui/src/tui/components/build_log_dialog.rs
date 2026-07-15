// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch docker-build log overlay helpers.

use jackin_tui::HintSpan;
use jackin_tui::components::{
    bottom_chrome_areas, is_scrollable, render_hint_bar, render_scrollable_block,
    scrollbar_offset_for_track_position, vertical_scrollbar_area, viewport_height, viewport_width,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear};
use termrock::style::DIALOG_SURFACE;

use crate::LaunchView;
use crate::tui::components::cells::coalesce_cells;
use crate::tui::components::footer::{launch_overlay_chrome_areas, render_footer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildLogScrollMetrics {
    pub content_len: usize,
    pub viewport_h: usize,
    pub filled: usize,
}

#[must_use]
pub const fn build_log_box_area(area: Rect) -> Rect {
    // Structural exception: build-log geometry is the shared bottom-chrome body, not an independent modal rect.
    bottom_chrome_areas(area).body
}

#[must_use]
pub fn build_log_scroll_metrics(area: Rect, raw: &[String]) -> BuildLogScrollMetrics {
    let box_area = build_log_box_area(area);
    let viewport_w = viewport_width(box_area);
    let viewport_h = viewport_height(box_area);
    let content_len = build_log_wrapped_lines(raw, viewport_w).len();
    BuildLogScrollMetrics {
        content_len,
        viewport_h,
        filled: termrock::scroll::max_offset(content_len, viewport_h),
    }
}

#[must_use]
pub fn build_log_wrapped_lines(raw: &[String], width: usize) -> Vec<Line<'static>> {
    if raw.is_empty() {
        vec![Line::from(Span::styled(
            "(waiting for docker build output…)",
            termrock::style::DIM,
        ))]
    } else {
        wrap_build_log_lines(raw, width)
    }
}

pub fn refresh_build_log_layout(view: &mut LaunchView, area: Rect, force: bool) {
    let box_area = build_log_box_area(area);
    let viewport_w = viewport_width(box_area);
    let viewport_h = viewport_height(box_area);
    if !force
        && view.build_log_wrapped_width == viewport_w
        && view.build_log_viewport_height == viewport_h
        && !view.build_log_wrapped_lines.is_empty()
    {
        return;
    }
    let wrapped = build_log_wrapped_lines(&view.build_log_lines, viewport_w);
    view.build_log_filled = termrock::scroll::max_offset(wrapped.len(), viewport_h);
    view.build_log_wrapped_lines = wrapped;
    view.build_log_wrapped_width = viewport_w;
    view.build_log_viewport_height = viewport_h;
}

#[must_use]
pub fn build_log_scroll_filled_for_lines(area: Rect, raw: &[String]) -> usize {
    build_log_scroll_metrics(area, raw).filled
}

#[must_use]
pub fn build_log_scrollbar_top_offset_at(
    area: Rect,
    raw: &[String],
    col: u16,
    row: u16,
) -> Option<usize> {
    let box_area = build_log_box_area(area);
    let scrollbar = vertical_scrollbar_area(box_area);
    if col < scrollbar.x
        || col >= scrollbar.x.saturating_add(scrollbar.width)
        || row < scrollbar.y
        || row >= scrollbar.y.saturating_add(scrollbar.height)
    {
        return None;
    }
    build_log_scrollbar_top_offset_for_row(area, raw, row)
}

#[must_use]
pub fn build_log_scrollbar_top_offset_for_row(
    area: Rect,
    raw: &[String],
    row: u16,
) -> Option<usize> {
    let metrics = build_log_scroll_metrics(area, raw);
    if !is_scrollable(metrics.content_len, metrics.viewport_h) {
        return None;
    }
    let scrollbar = vertical_scrollbar_area(build_log_box_area(area));
    let track_len = usize::from(scrollbar.height);
    if track_len == 0 {
        return None;
    }
    let max_position = scrollbar.height.saturating_sub(1);
    let track_position = row.saturating_sub(scrollbar.y).min(max_position);
    Some(usize::from(scrollbar_offset_for_track_position(
        metrics.content_len,
        metrics.viewport_h,
        track_len,
        usize::from(track_position),
    )))
}

#[must_use]
pub fn build_log_scrollbar_top_offset_for_row_cached(
    view: &LaunchView,
    area: Rect,
    col: u16,
    row: u16,
) -> Option<usize> {
    let box_area = build_log_box_area(area);
    let scrollbar = vertical_scrollbar_area(box_area);
    if col < scrollbar.x
        || col >= scrollbar.x.saturating_add(scrollbar.width)
        || row < scrollbar.y
        || row >= scrollbar.y.saturating_add(scrollbar.height)
    {
        return None;
    }
    if view.build_log_filled == 0 {
        return None;
    }
    let track_len = usize::from(scrollbar.height);
    if track_len == 0 {
        return None;
    }
    let max_position = scrollbar.height.saturating_sub(1);
    let track_position = row.saturating_sub(scrollbar.y).min(max_position);
    Some(usize::from(scrollbar_offset_for_track_position(
        view.build_log_wrapped_lines.len(),
        view.build_log_viewport_height,
        track_len,
        usize::from(track_position),
    )))
}

/// Footer-hint keys for the build-log overlay.
///
/// Delegates to [`crate::tui::keymap::build_log_hint_spans`] so hints and
/// dispatch stay coupled in the same module.
fn build_log_hint(vertical: bool) -> Vec<HintSpan<'static>> {
    crate::tui::keymap::build_log_hint_spans(vertical)
}

/// Full-screen opaque overlay over the live docker-build output, scrollable.
/// Opened by clicking the footer activity; dismissed by `Esc`.
/// Plain body clicks are swallowed; scrollbar hits remain interactive.
/// Long lines wrap inside the modal instead of requiring horizontal scroll;
/// continuation rows carry a visible prefix so wrapped Docker output remains
/// easy to distinguish from separate log lines. The key hint renders in the
/// bottom chrome area with the standard hint → blank separator → status footer
/// spacing, never inside the box (TUI design rule).
pub fn render_build_log_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &LaunchView,
    run_id: &str,
    debug_mode: bool,
) {
    frame.render_widget(
        Block::default().style(Style::default().bg(termrock::style::DIALOG_BACKDROP)),
        area,
    );
    let chrome = launch_overlay_chrome_areas(area, debug_mode);
    let box_area = chrome.body;

    let title = if view.build_log_active {
        " Docker build · building… "
    } else {
        " Docker build "
    };
    let lines: Vec<Line<'_>> = view.build_log_wrapped_lines.clone();

    // Live build output is tail-relative (0 = follow newest), unlike ordinary
    // top-offset panels that can use `apply_scroll_delta` directly. Keep the
    // state in the shared `TailScroll` adapter, then convert to the top-offset
    // consumed by `render_scrollable_block`/`FixedScrollbar`.
    let viewport_h = viewport_height(box_area);
    let lines_len = lines.len();
    let mut scroll_y = u16::try_from(view.build_log_scroll.to_top_offset(lines_len, viewport_h))
        .unwrap_or(u16::MAX);
    let mut scroll_x = 0u16;
    render_scrollable_block(
        frame,
        box_area,
        lines,
        &mut scroll_x,
        &mut scroll_y,
        true,
        Some(title),
    );

    let vertical = is_scrollable(lines_len, viewport_h);
    if !debug_mode {
        frame.render_widget(Clear, chrome.hint);
    }
    render_hint_bar(frame, chrome.hint, &build_log_hint(vertical));
    if debug_mode {
        render_footer(frame, chrome.footer, view, run_id, true);
    }
}

pub const BUILD_LOG_WRAP_PREFIX: &str = "↳ ";

#[must_use]
pub fn wrap_build_log_lines(raw: &[String], width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    raw.iter()
        .flat_map(|line| wrap_build_log_line(line, width))
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
                termrock::style::DIM.bg(DIALOG_SURFACE),
            ),
        );
    }
    lines.push(Line::from(spans));
}

#[cfg(test)]
mod tests;
