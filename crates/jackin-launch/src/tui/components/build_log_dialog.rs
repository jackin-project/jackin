//! Launch docker-build log overlay helpers.

use jackin_tui::HintSpan;
use jackin_tui::components::{
    ScrollAxes, is_scrollable, render_hint_bar, render_scrollable_block, scroll_hint_spans,
    scrollbar_offset_for_track_position, vertical_scrollbar_area, viewport_height, viewport_width,
};
use jackin_tui::theme::DIALOG_SURFACE;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Block;

use crate::LaunchView;
use crate::tui::components::footer::render_footer;

const BUILD_LOG_BOTTOM_ROWS: u16 = 3;
const BUILD_LOG_HINT_ROW_FROM_BOTTOM: u16 = 3;
const BUILD_LOG_FOOTER_ROW_FROM_BOTTOM: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildLogScrollMetrics {
    pub content_len: usize,
    pub viewport_h: usize,
    pub filled: usize,
}

#[must_use]
pub const fn build_log_box_area(area: Rect) -> Rect {
    Rect {
        height: area.height.saturating_sub(BUILD_LOG_BOTTOM_ROWS),
        ..area
    }
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
        filled: jackin_tui::scroll::max_offset(content_len, viewport_h),
    }
}

#[must_use]
pub fn build_log_wrapped_lines(raw: &[String], width: usize) -> Vec<Line<'static>> {
    if raw.is_empty() {
        vec![Line::from(Span::styled(
            "(waiting for docker build output…)",
            jackin_tui::theme::DIM,
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
    view.build_log_filled = jackin_tui::scroll::max_offset(wrapped.len(), viewport_h);
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

/// Footer-hint keys for the build-log overlay. The scroll + page keys appear
/// only when the wrapped output overflows the viewport (`vertical`) — when the
/// log fits, the overlay shows just "Esc close" rather than advertising a
/// scroll the operator cannot perform. The body is vertical-only (long lines
/// wrap), so there is never a horizontal-scroll hint.
fn build_log_hint(vertical: bool) -> Vec<HintSpan<'static>> {
    let mut spans = scroll_hint_spans(ScrollAxes {
        vertical,
        horizontal: false,
    });
    if vertical {
        spans.extend([
            HintSpan::GroupSep,
            HintSpan::Key("PgUp/PgDn"),
            HintSpan::Text("page"),
            HintSpan::GroupSep,
        ]);
    }
    spans.extend([HintSpan::Key("Esc"), HintSpan::Text("close")]);
    spans
}

/// Full-screen opaque overlay over the live docker-build output, scrollable.
/// Opened by clicking the footer activity; dismissed by `Esc`/`q` or a click.
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
        Block::default().style(Style::default().bg(jackin_tui::theme::DIALOG_BACKDROP)),
        area,
    );
    let footer_area = Rect {
        y: area.y + area.height.saturating_sub(BUILD_LOG_FOOTER_ROW_FROM_BOTTOM),
        height: 1,
        ..area
    };
    let hint_area = Rect {
        y: area.y + area.height.saturating_sub(BUILD_LOG_HINT_ROW_FROM_BOTTOM),
        height: 1,
        ..area
    };
    let box_area = build_log_box_area(area);

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
    render_hint_bar(frame, hint_area, &build_log_hint(vertical));
    render_footer(frame, footer_area, view, run_id, debug_mode);
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
                jackin_tui::theme::DIM.bg(DIALOG_SURFACE),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{LaunchIdentity, LaunchTargetKind};
    use ratatui::{Terminal, backend::TestBackend};

    fn row_text(buf: &ratatui::buffer::Buffer, row: u16, width: u16) -> String {
        (0..width)
            .map(|col| buf[(col, row)].symbol().to_owned())
            .collect::<String>()
    }

    #[test]
    fn scrollbar_hit_maps_track_to_top_offset() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 12,
        };
        let raw: Vec<String> = (0..20).map(|idx| format!("line {idx}")).collect();
        let scrollbar = vertical_scrollbar_area(build_log_box_area(area));

        let top = build_log_scrollbar_top_offset_at(area, &raw, scrollbar.x, scrollbar.y)
            .expect("top of scrollable track should hit");
        let bottom = build_log_scrollbar_top_offset_at(
            area,
            &raw,
            scrollbar.x,
            scrollbar.y + scrollbar.height.saturating_sub(1),
        )
        .expect("bottom of scrollable track should hit");

        assert_eq!(top, 0);
        assert!(bottom > top);
    }

    #[test]
    fn scrollbar_hit_ignores_non_track_columns() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 12,
        };
        let raw: Vec<String> = (0..20).map(|idx| format!("line {idx}")).collect();
        let scrollbar = vertical_scrollbar_area(build_log_box_area(area));

        assert_eq!(
            build_log_scrollbar_top_offset_at(
                area,
                &raw,
                scrollbar.x.saturating_sub(1),
                scrollbar.y
            ),
            None
        );
    }

    #[test]
    fn build_log_overlay_keeps_status_footer_in_debug_mode() {
        let area = Rect::new(0, 0, 80, 12);
        let mut view = crate::tui::update::initial_view();
        view.build_log_open = true;
        view.build_log_active = true;
        view.frame = 30;
        view.status = "building docker image".to_owned();
        view.identity = Some(LaunchIdentity {
            role: "the-architect".to_owned(),
            agent: "claude".to_owned(),
            target_kind: LaunchTargetKind::Directory,
            target_label: ".".to_owned(),
            mounts: Vec::new(),
            image: None,
            container: Some("jk-2y0t4aw6-the-architect".to_owned()),
        });
        view.build_log_lines = (0..30).map(|idx| format!("line {idx}")).collect();
        refresh_build_log_layout(&mut view, area, true);

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend should initialize");
        terminal
            .draw(|frame| render_build_log_dialog(frame, area, &view, "jk-run-c46709", true))
            .expect("render should succeed");

        let hint = row_text(terminal.backend().buffer(), area.height - 3, area.width);
        let separator = row_text(terminal.backend().buffer(), area.height - 2, area.width);
        let footer = row_text(terminal.backend().buffer(), area.height - 1, area.width);
        assert!(
            hint.contains("Esc"),
            "hint row should stay above separator and footer: {hint:?}"
        );
        assert!(
            !separator.contains("Esc")
                && !separator.contains("jk-run-c46709")
                && !separator.contains("2y0t4aw6"),
            "separator row should stay visually empty between hint and footer: {separator:?}"
        );
        assert!(
            footer.contains("jk-run-c46709"),
            "debug footer should stay visible while build log is open: {footer:?}"
        );
        assert!(
            footer.contains("2y0t4aw6"),
            "instance footer should stay visible while build log is open: {footer:?}"
        );
    }
}
