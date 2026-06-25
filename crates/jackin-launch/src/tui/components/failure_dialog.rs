//! Launch failure popup rendering and hit-testing.

use jackin_tui::components::{ModalBackdrop, render_hint_bar};
use jackin_tui::theme::{DANGER_RED, LINK_BLUE, PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE};
use jackin_tui::{HintSpan, centered_rect};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::tui::components::footer::launch_overlay_chrome_areas;
use crate::{FailureCopyTarget, LaunchFailure, LaunchView};

#[derive(Debug)]
pub struct FailurePopupRow {
    label: &'static str,
    value: String,
    copy_target: Option<FailureCopyTarget>,
    href: Option<String>,
}

#[must_use]
pub fn failure_popup_rows(failure: &LaunchFailure, run_id: &str) -> Vec<FailurePopupRow> {
    let mut rows = vec![
        FailurePopupRow {
            label: "message",
            value: failure.summary.clone(),
            copy_target: None,
            href: None,
        },
        FailurePopupRow {
            label: "stage",
            value: failure.stage.label().to_owned(),
            copy_target: None,
            href: None,
        },
        FailurePopupRow {
            label: "run id",
            value: run_id.to_owned(),
            copy_target: Some(FailureCopyTarget::RunId),
            href: None,
        },
    ];
    if let Some(path) = &failure.diagnostics_path {
        let value = path.display().to_string();
        rows.push(FailurePopupRow {
            label: "run diagnostics",
            href: Some(format!("file://{value}")),
            value,
            copy_target: Some(FailureCopyTarget::DiagnosticsPath),
        });
    }
    if let Some(path) = &failure.command_output_path {
        let value = path.display().to_string();
        rows.push(FailurePopupRow {
            label: "docker output",
            href: Some(format!("file://{value}")),
            value,
            copy_target: Some(FailureCopyTarget::CommandOutputPath),
        });
    }
    if let Some(next) = &failure.next_step {
        rows.push(FailurePopupRow {
            label: "next",
            value: next.clone(),
            copy_target: None,
            href: None,
        });
    }
    rows
}

fn failure_popup_rect(area: Rect, row_count: usize) -> Rect {
    // Structural exception: failure popup height depends on wrapped diagnostic rows and copy targets before centering.
    let popup_w = (area.width.saturating_mul(3) / 5)
        .clamp(40.min(area.width), area.width.saturating_sub(2).max(1));
    // Height breakdown: border(2) + body rows + 1 empty separator + button(1) + bottom-pad(1)
    // = row_count + 5. This gives exactly one blank row between the last data row and OK.
    let height = u16::try_from(row_count)
        .unwrap_or(u16::MAX)
        .saturating_add(5)
        .min(area.height.saturating_sub(2).max(7));
    centered_rect(popup_w, height, area)
}

#[must_use]
pub fn failure_popup_rect_for_rows(area: Rect, rows: &[FailurePopupRow]) -> Rect {
    let popup_w = (area.width.saturating_mul(3) / 5)
        .clamp(40.min(area.width), area.width.saturating_sub(2).max(1));
    let probe = centered_rect(popup_w, area.height.saturating_sub(2).max(7), area);
    let body = failure_popup_body_rect(probe);
    failure_popup_rect(area, failure_popup_render_line_count(rows, body.width))
}

fn failure_popup_render_line_count(rows: &[FailurePopupRow], width: u16) -> usize {
    rows.iter()
        .map(|row| failure_popup_value_chunks(row, width, None, None, None).len())
        .sum::<usize>()
        .max(1)
}

fn failure_popup_value_chunks(
    row: &FailurePopupRow,
    width: u16,
    copied: Option<FailureCopyTarget>,
    revealed: Option<FailureCopyTarget>,
    opened: Option<FailureCopyTarget>,
) -> Vec<String> {
    let badge = match row.copy_target {
        Some(target) if copied == Some(target) => "  Copied!",
        Some(target) if revealed == Some(target) => "  Revealed!",
        Some(target) if opened == Some(target) => "  Opened!",
        _ => "",
    };
    let first_fixed_cols = FAILURE_POPUP_LABEL_WIDTH
        + jackin_tui::display_cols(FAILURE_POPUP_SEP)
        + jackin_tui::display_cols(badge);
    let continuation_fixed_cols =
        FAILURE_POPUP_LABEL_WIDTH + jackin_tui::display_cols(FAILURE_POPUP_SEP);
    let first_cols = usize::from(width).saturating_sub(first_fixed_cols).max(1);
    let continuation_cols = usize::from(width)
        .saturating_sub(continuation_fixed_cols)
        .max(1);
    let mut rest = row.value.as_str();
    let mut chunks = Vec::new();
    let mut cols = first_cols;
    while !rest.is_empty() {
        let chunk = jackin_tui::take_display_cols(rest, cols);
        if chunk.is_empty() {
            break;
        }
        rest = &rest[chunk.len()..];
        chunks.push(chunk);
        cols = continuation_cols;
    }
    if chunks.is_empty() {
        chunks.push(String::new());
    }
    chunks
}

/// Inner body rect (inside the border, plus one column of padding) where the
/// failure rows render. Render and hit-testing derive geometry from this same
/// helper so the clickable value columns can never drift from what is drawn.
const fn failure_popup_body_rect(rect: Rect) -> Rect {
    // Structural exception: render and hit-testing share this value-cell body rect so copy targets cannot drift.
    let inner = rect.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    Rect {
        x: inner.x.saturating_add(1),
        y: inner.y.saturating_add(1),
        width: inner.width.saturating_sub(2),
        height: inner.height.saturating_sub(3),
    }
}

#[must_use]
pub fn failure_popup_value_rect(
    rect: Rect,
    rows: &[FailurePopupRow],
    target: FailureCopyTarget,
) -> Option<Rect> {
    // Structural exception: copy hit-testing derives rects from wrapped failure rows rendered by this dialog.
    failure_popup_value_rects(rect, rows, target)
        .into_iter()
        .next()
}

fn failure_popup_value_rects(
    rect: Rect,
    rows: &[FailurePopupRow],
    target: FailureCopyTarget,
) -> Vec<Rect> {
    let body = failure_popup_body_rect(rect);
    let x = body.x.saturating_add(
        u16::try_from(FAILURE_POPUP_LABEL_WIDTH + jackin_tui::display_cols(FAILURE_POPUP_SEP))
            .unwrap_or(u16::MAX),
    );
    let mut y = body.y;
    let mut rects = Vec::new();
    for row in rows {
        let chunks = failure_popup_value_chunks(row, body.width, None, None, None);
        if row.copy_target == Some(target) {
            for chunk in &chunks {
                if y >= body.y.saturating_add(body.height) {
                    break;
                }
                let width = u16::try_from(jackin_tui::display_cols(chunk))
                    .unwrap_or(u16::MAX)
                    .max(1);
                rects.push(Rect {
                    x,
                    y,
                    width: body
                        .x
                        .saturating_add(body.width)
                        .saturating_sub(x)
                        .min(width),
                    height: 1,
                });
                y = y.saturating_add(1);
            }
        } else {
            y = y.saturating_add(u16::try_from(chunks.len()).unwrap_or(u16::MAX));
        }
        if y >= body.y.saturating_add(body.height) {
            break;
        }
    }
    rects
}

#[must_use]
pub fn failure_copy_target_at(
    area: Rect,
    failure: &LaunchFailure,
    run_id: &str,
    debug_mode: bool,
    col: u16,
    row: u16,
) -> Option<FailureCopyTarget> {
    let body_area = launch_overlay_chrome_areas(area, debug_mode).body;
    let rows = failure_popup_rows(failure, run_id);
    let rect = failure_popup_rect_for_rows(body_area, &rows);
    for entry in rows.iter().filter(|row| row.copy_target.is_some()) {
        let target = entry.copy_target?;
        for value_rect in failure_popup_value_rects(rect, &rows, target) {
            if row == value_rect.y
                && col >= value_rect.x
                && col < value_rect.x.saturating_add(value_rect.width)
            {
                return Some(target);
            }
        }
    }
    None
}

#[must_use]
pub fn failure_copy_payload(
    failure: &LaunchFailure,
    run_id: &str,
    target: FailureCopyTarget,
) -> Option<String> {
    // Derive the copied value from the same `failure_popup_rows` builder the
    // renderer uses. Re-deriving paths/run-id here would duplicate the
    // formatting logic and drift if `failure_popup_rows` ever changes how it
    // displays a path (shell-escaping, `~`-collapse, etc.).
    failure_popup_rows(failure, run_id)
        .into_iter()
        .find(|row| row.copy_target == Some(target))
        .map(|row| row.value)
}

#[must_use]
pub fn failure_reveal_payload(
    failure: &LaunchFailure,
    run_id: &str,
    preferred: Option<FailureCopyTarget>,
) -> Option<(FailureCopyTarget, String)> {
    let rows = failure_popup_rows(failure, run_id);
    let revealable = |target: FailureCopyTarget| {
        matches!(
            target,
            FailureCopyTarget::DiagnosticsPath | FailureCopyTarget::CommandOutputPath
        )
    };
    if let Some(target) = preferred.filter(|target| revealable(*target))
        && let Some(value) = rows
            .iter()
            .find(|row| row.copy_target == Some(target))
            .map(|row| row.value.clone())
    {
        return Some((target, value));
    }
    rows.into_iter()
        .filter_map(|row| row.copy_target.map(|target| (target, row.value)))
        .find(|(target, _)| revealable(*target))
}

fn render_failure_popup_lines(
    row: &FailurePopupRow,
    width: u16,
    hovered: Option<FailureCopyTarget>,
    copied: Option<FailureCopyTarget>,
    revealed: Option<FailureCopyTarget>,
    opened: Option<FailureCopyTarget>,
) -> Vec<Line<'static>> {
    let label = jackin_tui::theme::DIM;
    let value_style = match row.copy_target {
        Some(target) if hovered == Some(target) => Style::default()
            .fg(LINK_BLUE)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        Some(_) => jackin_tui::theme::BOLD_WHITE,
        None => Style::default().fg(WHITE),
    };
    let label_width = FAILURE_POPUP_LABEL_WIDTH;
    let badge = match row.copy_target {
        Some(target) if copied == Some(target) => "  Copied!",
        Some(target) if revealed == Some(target) => "  Revealed!",
        Some(target) if opened == Some(target) => "  Opened!",
        _ => "",
    };
    failure_popup_value_chunks(row, width, copied, revealed, opened)
        .into_iter()
        .enumerate()
        .map(|(idx, value)| {
            let row_label = if idx == 0 { row.label } else { "" };
            let mut spans = vec![
                Span::styled(format!("{row_label:<label_width$}"), label),
                Span::styled(FAILURE_POPUP_SEP, Style::default().fg(PHOSPHOR_DARK)),
                Span::styled(value, value_style),
            ];
            if idx == 0 && !badge.is_empty() {
                spans.push(Span::styled(
                    badge,
                    Style::default()
                        .fg(PHOSPHOR_GREEN)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            Line::from(spans)
        })
        .collect()
}

const FAILURE_POPUP_LABEL_WIDTH: usize = 16;
/// Separator drawn between a row's label and value. The renderer paints
/// this string and the click hit-test uses its display width as the
/// label→value column offset, so the two cannot drift if the separator
/// is ever changed.
const FAILURE_POPUP_SEP: &str = " · ";

pub fn render_failure_popup(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &LaunchView,
    failure: &LaunchFailure,
    run_id: &str,
    debug_mode: bool,
) {
    let chrome = launch_overlay_chrome_areas(area, debug_mode);
    frame.render_widget(ModalBackdrop, chrome.body);

    let rows = failure_popup_rows(failure, run_id);
    let rect = failure_popup_rect_for_rows(chrome.body, &rows);
    let title = format!(" {} ", failure.title);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DANGER_RED))
        .title(Span::styled(title, jackin_tui::theme::DANGER));
    let inner = block.inner(rect);
    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    let body = failure_popup_body_rect(rect);
    let lines = rows
        .iter()
        .flat_map(|row| {
            render_failure_popup_lines(
                row,
                body.width,
                view.failure_copy_hover,
                view.failure_copied,
                view.failure_revealed,
                view.failure_opened,
            )
        })
        .collect::<Vec<_>>();
    for (idx, line) in lines.iter().take(usize::from(body.height)).enumerate() {
        let row_area = Rect {
            x: body.x,
            y: body.y + u16::try_from(idx).unwrap_or(u16::MAX),
            width: body.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(line.clone()), row_area);
    }

    let focused_style = Style::default()
        .bg(WHITE)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let button_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1),
        width: inner.width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("  OK  ", focused_style)))
            .alignment(Alignment::Center),
        button_area,
    );
    // The popup draws no hint of its own; keys live in the shared hint row.
    // In non-debug overlays that row replaces the base footer, so clear first
    // or a shorter hint can leave stale right-side footer text behind.
    if !debug_mode {
        frame.render_widget(Clear, chrome.hint);
    }
    render_hint_bar(frame, chrome.hint, FAILURE_HINT);
}

#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn failure_popup_hyperlink_overlay(
    area: Rect,
    failure: &LaunchFailure,
    run_id: &str,
    debug_mode: bool,
    hovered: Option<FailureCopyTarget>,
    copied: Option<FailureCopyTarget>,
    revealed: Option<FailureCopyTarget>,
    opened: Option<FailureCopyTarget>,
) -> Vec<u8> {
    let body_area = launch_overlay_chrome_areas(area, debug_mode).body;
    let rows = failure_popup_rows(failure, run_id);
    let rect = failure_popup_rect_for_rows(body_area, &rows);
    let body = failure_popup_body_rect(rect);
    let x = body.x.saturating_add(
        u16::try_from(FAILURE_POPUP_LABEL_WIDTH + jackin_tui::display_cols(FAILURE_POPUP_SEP))
            .unwrap_or(u16::MAX),
    );
    let mut y = body.y;
    let mut out = Vec::new();
    for row in &rows {
        let chunks = failure_popup_value_chunks(row, body.width, copied, revealed, opened);
        if let Some(href) = row.href.as_deref() {
            for chunk in &chunks {
                if y >= body.y.saturating_add(body.height) {
                    break;
                }
                jackin_tui::ansi::move_to(&mut out, y, x);
                out.extend_from_slice(jackin_tui::ansi::RESET.as_bytes());
                jackin_tui::ansi::fg(&mut out, jackin_tui::LINK_BLUE);
                out.extend_from_slice(jackin_tui::ansi::BOLD.as_bytes());
                if hovered == row.copy_target {
                    out.extend_from_slice(b"\x1b[4m");
                }
                jackin_tui::ansi::emit_osc8_open(&mut out, href);
                out.extend_from_slice(chunk.as_bytes());
                jackin_tui::ansi::emit_osc8_close(&mut out);
                out.extend_from_slice(jackin_tui::ansi::RESET.as_bytes());
                y = y.saturating_add(1);
            }
        } else {
            y = y.saturating_add(u16::try_from(chunks.len()).unwrap_or(u16::MAX));
        }
        if y >= body.y.saturating_add(body.height) {
            break;
        }
    }
    out
}

/// Footer-hint keys for the launch failure popup (dismiss only).
const FAILURE_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("click"),
    HintSpan::Text("copy value"),
    HintSpan::GroupSep,
    HintSpan::Key("r"),
    HintSpan::Text("reveal file"),
    HintSpan::GroupSep,
    HintSpan::Key("o"),
    HintSpan::Text("open file"),
    HintSpan::GroupSep,
    HintSpan::Key("↵/Esc"),
    HintSpan::Text("dismiss"),
];

#[cfg(test)]
mod tests {
    use crate::tui::app::{LaunchIdentity, LaunchTargetKind};
    use crate::tui::update::initial_view;
    use crate::tui::view::render_launch_frame;
    use crate::{LaunchStage, tui::app::LaunchFailure};
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};

    fn row_text(buf: &Buffer, row: u16, width: u16) -> String {
        (0..width)
            .map(|x| buf[(x, row)].symbol().to_owned())
            .collect()
    }

    #[test]
    fn failure_popup_keeps_status_footer_visible() {
        let area = Rect::new(0, 0, 90, 18);
        let mut view = initial_view();
        view.frame = 30;
        view.status = "docker build failed".to_owned();
        view.identity = Some(LaunchIdentity {
            role: "the-architect".to_owned(),
            agent: "claude".to_owned(),
            target_kind: LaunchTargetKind::Directory,
            target_label: ".".to_owned(),
            mounts: Vec::new(),
            image: None,
            container: Some("jk-2y0t4aw6-the-architect".to_owned()),
        });
        view.failure = Some(LaunchFailure {
            title: "Build failed".to_owned(),
            summary: "docker build failed".to_owned(),
            detail: None,
            next_step: None,
            stage: LaunchStage::DerivedImage,
            diagnostics_path: None,
            command_output_path: None,
        });

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend should initialize");
        terminal
            .draw(|frame| {
                render_launch_frame(
                    frame,
                    &view,
                    "jk-run-c46709",
                    "/tmp/jk-run-c46709.jsonl",
                    true,
                    None,
                    true,
                    "0.6.0-test",
                );
            })
            .expect("render should succeed");

        let hint = row_text(terminal.backend().buffer(), area.height - 3, area.width);
        let spacer = row_text(terminal.backend().buffer(), area.height - 2, area.width);
        let footer = row_text(terminal.backend().buffer(), area.height - 1, area.width);
        assert!(
            hint.contains("copy value") && hint.contains("dismiss"),
            "failure popup hints should render in the shared hint row: {hint:?}"
        );
        assert!(
            !spacer.contains("copy value") && !spacer.contains("jk-run-c46709"),
            "spacer row should stay between hints and footer: {spacer:?}"
        );
        assert!(
            footer.contains("jk-run-c46709") && footer.contains("2y0t4aw6"),
            "status footer should remain visible while failure popup is open: {footer:?}"
        );
    }
}
