//! Launch docker-build log overlay helpers.

use jackin_tui::HintSpan;
use jackin_tui::components::{
    ScrollAxes, is_scrollable, render_hint_bar, render_scrollable_block, scroll_hint_spans,
    viewport_height, viewport_width,
};
use jackin_tui::theme::DIALOG_SURFACE;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::LaunchView;
use crate::tui::components::dialog::dialog_backdrop;

#[must_use]
pub fn build_log_scroll_filled_for_lines(area: Rect, raw: &[String]) -> usize {
    let box_area = Rect {
        height: area.height.saturating_sub(1),
        ..area
    };
    let viewport_w = viewport_width(box_area);
    let viewport_h = viewport_height(box_area);
    let line_count = if raw.is_empty() {
        1
    } else {
        wrap_build_log_lines(raw.to_vec(), viewport_w).len()
    };
    jackin_tui::scroll::max_offset(line_count, viewport_h)
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
/// bottom footer row, never inside the box (TUI design rule).
pub fn render_build_log_dialog(frame: &mut Frame<'_>, area: Rect, view: &LaunchView) {
    let (box_area, hint_area) = dialog_backdrop(frame, area);

    let title = if view.build_log_active {
        " Docker build · building… "
    } else {
        " Docker build "
    };
    // The full output drives the shared scrollable block so its proportional
    // scrollbar is correct. Cloning the (capped) buffer is acceptable here: the
    // overlay is a transient, operator-opened modal, not the steady cockpit.
    let raw = view.build_log_lines.clone();
    let viewport_w = viewport_width(box_area);
    let lines: Vec<Line<'_>> = if raw.is_empty() {
        vec![Line::from(Span::styled(
            "(waiting for docker build output…)",
            jackin_tui::theme::DIM,
        ))]
    } else {
        wrap_build_log_lines(raw, viewport_w)
    };

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
