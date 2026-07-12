//! Launch failure popup rendering and hit-testing.

use jackin_tui::HintSpan;
use jackin_tui::components::{
    ErrorPopupRow, ErrorPopupState, ModalBackdrop, ModalRectSpec, dialog_inner_chunks,
    error_popup_hyperlink_overlay, error_popup_row_value_rect_groups, modal_rect,
    render_error_dialog_in, render_hint_bar, required_height,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Clear;

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
    if let Some(query) = jackin_diagnostics::backend_query_hint(run_id) {
        rows.push(FailurePopupRow {
            label: "backend query",
            value: query,
            copy_target: None,
            href: None,
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

fn failure_error_state(
    failure: &LaunchFailure,
    run_id: &str,
    view: Option<&LaunchView>,
) -> ErrorPopupState {
    failure_error_state_with_feedback(
        failure,
        run_id,
        view.map(|view| view.failure_scroll.clone()),
        view.and_then(|view| view.failure_copy_hover),
        view.and_then(|view| view.failure_copied),
        view.and_then(|view| view.failure_revealed),
        view.and_then(|view| view.failure_opened),
    )
}

fn failure_error_state_with_feedback(
    failure: &LaunchFailure,
    run_id: &str,
    scroll: Option<jackin_tui::components::DialogBodyScroll>,
    hovered: Option<FailureCopyTarget>,
    copied: Option<FailureCopyTarget>,
    revealed: Option<FailureCopyTarget>,
    opened: Option<FailureCopyTarget>,
) -> ErrorPopupState {
    let rows = failure_popup_rows(failure, run_id)
        .into_iter()
        .filter(|row| row.label != "message")
        .map(|row| {
            let mut display =
                ErrorPopupRow::new(row.label, row.value).strong(row.copy_target.is_some());
            if let Some(href) = row.href {
                display = display.hyperlink(href);
            }
            if let Some(target) = row.copy_target {
                display = display.highlighted(hovered == Some(target));
                let badge = match target {
                    _ if copied == Some(target) => Some("Copied!"),
                    _ if revealed == Some(target) => Some("Revealed!"),
                    _ if opened == Some(target) => Some("Opened!"),
                    _ => None,
                };
                if let Some(badge) = badge {
                    display = display.badge(badge);
                }
            }
            display
        })
        .collect::<Vec<_>>();
    let mut state =
        ErrorPopupState::new(failure.title.clone(), failure.summary.clone()).with_rows(rows);
    if let Some(scroll) = scroll {
        state.scroll = scroll;
    }
    state
}

fn failure_popup_rect(area: Rect, state: &ErrorPopupState) -> Rect {
    let popup_w = (area.width.saturating_mul(4) / 5)
        .clamp(40.min(area.width), area.width.saturating_sub(2).max(1));
    let height = required_height(
        state,
        popup_w.saturating_sub(2),
        area.height.saturating_sub(2).max(7),
    );
    modal_rect(
        area,
        ModalRectSpec::Exact {
            width: popup_w,
            height,
        },
    )
}

#[must_use]
pub fn failure_popup_rect_for_rows(area: Rect, rows: &[FailurePopupRow]) -> Rect {
    let display_rows = rows
        .iter()
        .filter(|row| row.label != "message")
        .map(|row| {
            let mut display =
                ErrorPopupRow::new(row.label, row.value.clone()).strong(row.copy_target.is_some());
            if let Some(href) = &row.href {
                display = display.hyperlink(href.clone());
            }
            display
        })
        .collect::<Vec<_>>();
    let message = rows
        .iter()
        .find(|row| row.label == "message")
        .map_or("", |row| row.value.as_str());
    let state = ErrorPopupState::new("Build failed", message).with_rows(display_rows);
    failure_popup_rect(area, &state)
}

/// Inner body rect (inside the border, plus one column of padding) where the
/// failure rows render. Render and hit-testing derive geometry from this same
/// helper so the clickable value columns can never drift from what is drawn.
fn failure_popup_body_rect(rect: Rect, state: &ErrorPopupState) -> Rect {
    let inner = rect.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    let content_rows = jackin_tui::components::estimated_message_rows(state, inner.width)
        .min(inner.height.saturating_sub(4));
    let chunks = dialog_inner_chunks(inner, Some(content_rows));
    chunks[1]
}

#[must_use]
pub fn failure_popup_value_rect(
    rect: Rect,
    rows: &[FailurePopupRow],
    target: FailureCopyTarget,
) -> Option<Rect> {
    failure_popup_value_rect_scrolled(rect, rows, target, None)
}

/// Value-column hit rects for `target`, using the same body scroll as render.
#[must_use]
pub fn failure_popup_value_rect_scrolled(
    rect: Rect,
    rows: &[FailurePopupRow],
    target: FailureCopyTarget,
    scroll: Option<jackin_tui::components::DialogBodyScroll>,
) -> Option<Rect> {
    // Structural exception: copy hit-testing derives rects from wrapped failure rows rendered by this dialog.
    failure_popup_value_rects(rect, rows, target, scroll)
        .into_iter()
        .next()
}

fn failure_popup_value_rects(
    rect: Rect,
    rows: &[FailurePopupRow],
    target: FailureCopyTarget,
    scroll: Option<jackin_tui::components::DialogBodyScroll>,
) -> Vec<Rect> {
    let display_rows = rows
        .iter()
        .filter(|row| row.label != "message")
        .map(|row| {
            let mut display =
                ErrorPopupRow::new(row.label, row.value.clone()).strong(row.copy_target.is_some());
            if let Some(href) = &row.href {
                display = display.hyperlink(href.clone());
            }
            display
        })
        .collect::<Vec<_>>();
    let message = rows
        .iter()
        .find(|row| row.label == "message")
        .map_or("", |row| row.value.as_str());
    let mut state = ErrorPopupState::new("Build failed", message).with_rows(display_rows);
    if let Some(scroll) = scroll {
        state.scroll = scroll;
    }
    let inner = rect.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    let target_idx = rows
        .iter()
        .filter(|row| row.label != "message")
        .position(|row| row.copy_target == Some(target));
    target_idx
        .and_then(|idx| {
            error_popup_row_value_rect_groups(inner, &state)
                .into_iter()
                .nth(idx)
        })
        .unwrap_or_default()
}

/// Hit-test a copy target at `(col, row)` using the same body scroll as render.
///
/// Pass the live `failure_scroll` so scrolled popups hit the visible rows;
/// `None` means scroll 0 (tests and scroll-independent callers).
#[must_use]
pub fn failure_copy_target_at(
    area: Rect,
    failure: &LaunchFailure,
    run_id: &str,
    debug_mode: bool,
    col: u16,
    row: u16,
    scroll: Option<jackin_tui::components::DialogBodyScroll>,
) -> Option<FailureCopyTarget> {
    let body_area = launch_overlay_chrome_areas(area, debug_mode).body;
    let state =
        failure_error_state_with_feedback(failure, run_id, scroll.clone(), None, None, None, None);
    let rect = failure_popup_rect(body_area, &state);
    let rows = failure_popup_rows(failure, run_id);
    for entry in rows.iter().filter(|row| row.copy_target.is_some()) {
        let target = entry.copy_target?;
        for value_rect in failure_popup_value_rects(rect, &rows, target, scroll.clone()) {
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

/// Outer block rect of the failure popup within `area`, so the input layer can
/// classify clicks (inside vs outside the modal) without re-deriving the
/// layout. Matches the rect `render_failure_popup` draws.
#[must_use]
pub fn failure_popup_block_rect(
    area: Rect,
    failure: &LaunchFailure,
    run_id: &str,
    debug_mode: bool,
) -> Rect {
    let body_area = launch_overlay_chrome_areas(area, debug_mode).body;
    let state = failure_error_state(failure, run_id, None);
    failure_popup_rect(body_area, &state)
}

/// `(body_rect, content_height)` for the failure popup body, so the input layer
/// scrolls long diagnostics/next-step rows against the same geometry the
/// renderer measures. `content_height` is the wrapped line count at the body
/// width; vertical scroll is reachable when it exceeds the body viewport.
#[must_use]
pub fn failure_popup_body_metrics(
    area: Rect,
    failure: &LaunchFailure,
    run_id: &str,
    debug_mode: bool,
) -> (Rect, usize) {
    let body_area = launch_overlay_chrome_areas(area, debug_mode).body;
    let state = failure_error_state(failure, run_id, None);
    let rect = failure_popup_rect(body_area, &state);
    let body = failure_popup_body_rect(rect, &state);
    let content_height = usize::from(jackin_tui::components::estimated_message_rows(
        &state, body.width,
    ));
    (body, content_height)
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

    let state = failure_error_state(failure, run_id, Some(view));
    let rect = failure_popup_rect(chrome.body, &state);
    render_error_dialog_in(frame, rect, &state);
    // The popup draws no hint of its own; keys live in the shared hint row.
    // In non-debug overlays that row replaces the base footer, so clear first
    // or a shorter hint can leave stale right-side footer text behind.
    if !debug_mode {
        frame.render_widget(Clear, chrome.hint);
    }
    render_hint_bar(frame, chrome.hint, &failure_hint_spans());
}

#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn failure_popup_hyperlink_overlay(
    area: Rect,
    failure: &LaunchFailure,
    run_id: &str,
    debug_mode: bool,
    scroll: Option<jackin_tui::components::DialogBodyScroll>,
    hovered: Option<FailureCopyTarget>,
    copied: Option<FailureCopyTarget>,
    revealed: Option<FailureCopyTarget>,
    opened: Option<FailureCopyTarget>,
) -> Vec<u8> {
    let body_area = launch_overlay_chrome_areas(area, debug_mode).body;
    let state = failure_error_state_with_feedback(
        failure, run_id, scroll, hovered, copied, revealed, opened,
    );
    let rect = failure_popup_rect(body_area, &state);
    let inner = rect.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    error_popup_hyperlink_overlay(inner, &state)
}

/// Footer-hint keys for the launch failure popup. The dismiss group derives
/// from `FAILURE_KEYMAP` (the dispatch table); the global keys derive from
/// `cockpit_global_hint_spans`. Only the mouse "click copy value" affordance is
/// authored here since it is not a key.
fn failure_hint_spans() -> Vec<HintSpan<'static>> {
    let mut spans = vec![
        // UNREGISTERABLE(mouse): mouse click cannot be expressed as a KeyChord.
        HintSpan::Key("click"),
        HintSpan::Text("copy value"),
        HintSpan::GroupSep,
    ];
    spans.extend(crate::tui::keymap::FAILURE_KEYMAP.hint_spans());
    spans.push(HintSpan::GroupSep);
    spans.extend(crate::tui::keymap::cockpit_global_hint_spans());
    spans
}

#[cfg(test)]
mod tests;
