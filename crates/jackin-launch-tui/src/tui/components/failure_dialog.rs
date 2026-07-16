// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch failure composition over TermRock's canonical message/detail widgets.

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::Text,
    widgets::{Clear, StatefulWidget},
};
use termrock::{
    HintSpan, Theme,
    widgets::{
        DetailCapability, DetailRow, DetailTableState, Dialog, MessageDialog, PanelEmphasis,
    },
};

use crate::tui::components::dialog::{exact_dialog_rect, render_dialog_backdrop};
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

fn message(rows: &[FailurePopupRow]) -> &str {
    rows.iter()
        .find(|row| row.label == "message")
        .map_or("", |row| row.value.as_str())
}

fn detail_rows(rows: &[FailurePopupRow]) -> Vec<DetailRow<'_, usize>> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.label != "message")
        .map(|(id, row)| DetailRow {
            id,
            label: row.label,
            value: &row.value,
            href: row.href.as_deref(),
            capability: match (row.copy_target.is_some(), row.href.is_some()) {
                (true, true) => DetailCapability::CopyAndLink,
                (true, false) => DetailCapability::Copy,
                (false, true) => DetailCapability::Link,
                (false, false) => DetailCapability::None,
            },
            emphasis: row.copy_target.is_some(),
            style: None,
        })
        .collect()
}

fn target_id(rows: &[FailurePopupRow], target: FailureCopyTarget) -> Option<usize> {
    rows.iter().position(|row| row.copy_target == Some(target))
}

fn popup_height(area: Rect, rows: &[FailurePopupRow]) -> u16 {
    let width = usize::from(area.width.saturating_sub(4)).max(1);
    let message_rows = message(rows)
        .lines()
        .map(|line| termrock::display_cols(line).div_ceil(width).max(1))
        .sum::<usize>();
    let detail_rows = rows
        .iter()
        .filter(|row| row.label != "message")
        .map(|row| termrock::display_cols(&row.value).div_ceil(width).max(1))
        .sum::<usize>();
    u16::try_from(message_rows.saturating_add(detail_rows).saturating_add(3))
        .unwrap_or(u16::MAX)
        .clamp(7.min(area.height), area.height.saturating_sub(2).max(1))
}

fn failure_popup_rect(area: Rect, rows: &[FailurePopupRow]) -> Rect {
    let width = (area.width.saturating_mul(4) / 5)
        .clamp(40.min(area.width), area.width.saturating_sub(2).max(1));
    exact_dialog_rect(area, width, popup_height(area, rows))
}

#[must_use]
pub fn failure_popup_rect_for_rows(area: Rect, rows: &[FailurePopupRow]) -> Rect {
    failure_popup_rect(area, rows)
}

fn layout_state(
    rect: Rect,
    title: &str,
    rows: &[FailurePopupRow],
    scroll: Option<termrock::scroll::DialogScroll>,
    hovered: Option<FailureCopyTarget>,
    copied: Option<FailureCopyTarget>,
) -> DetailTableState<usize> {
    let details = detail_rows(rows);
    let theme = Theme::default();
    let mut state = DetailTableState {
        hovered: hovered.and_then(|target| target_id(rows, target)),
        copied: copied.and_then(|target| target_id(rows, target)),
        scroll: scroll.unwrap_or_default(),
        ..DetailTableState::default()
    };
    let mut buffer = Buffer::empty(rect);
    StatefulWidget::render(
        &MessageDialog {
            dialog: Dialog {
                title,
                body: Text::from(message(rows)),
                style: Style::default(),
                theme: &theme,
                emphasis: PanelEmphasis::Focused,
            },
            details: &details,
            label_width: 0,
            wrap: true,
            theme: &theme,
        },
        rect,
        &mut buffer,
        &mut state,
    );
    state
}

#[must_use]
pub fn failure_popup_value_rect(
    rect: Rect,
    rows: &[FailurePopupRow],
    target: FailureCopyTarget,
) -> Option<Rect> {
    failure_popup_value_rect_scrolled(rect, rows, target, None)
}

#[must_use]
pub fn failure_popup_value_rect_scrolled(
    rect: Rect,
    rows: &[FailurePopupRow],
    target: FailureCopyTarget,
    scroll: Option<termrock::scroll::DialogScroll>,
) -> Option<Rect> {
    failure_popup_value_rects(rect, rows, target, scroll)
        .into_iter()
        .next()
}

fn failure_popup_value_rects(
    rect: Rect,
    rows: &[FailurePopupRow],
    target: FailureCopyTarget,
    scroll: Option<termrock::scroll::DialogScroll>,
) -> Vec<Rect> {
    let Some(id) = target_id(rows, target) else {
        return Vec::new();
    };
    layout_state(rect, "Build failed", rows, scroll, None, None)
        .regions
        .into_iter()
        .filter_map(|region| (region.id == id).then_some(region.value_area))
        .collect()
}

#[must_use]
pub fn failure_copy_target_at(
    area: Rect,
    failure: &LaunchFailure,
    run_id: &str,
    debug_mode: bool,
    col: u16,
    row: u16,
    scroll: Option<termrock::scroll::DialogScroll>,
) -> Option<FailureCopyTarget> {
    let body = launch_overlay_chrome_areas(area, debug_mode).body;
    let rows = failure_popup_rows(failure, run_id);
    let rect = failure_popup_rect(body, &rows);
    let state = layout_state(rect, &failure.title, &rows, scroll, None, None);
    state.regions.into_iter().find_map(|region| {
        if !region.action_area.contains((col, row).into()) {
            return None;
        }
        rows.get(region.id)?.copy_target
    })
}

#[must_use]
pub fn failure_popup_block_rect(
    area: Rect,
    failure: &LaunchFailure,
    run_id: &str,
    debug_mode: bool,
) -> Rect {
    let body = launch_overlay_chrome_areas(area, debug_mode).body;
    failure_popup_rect(body, &failure_popup_rows(failure, run_id))
}

#[must_use]
pub fn failure_popup_body_metrics(
    area: Rect,
    failure: &LaunchFailure,
    run_id: &str,
    debug_mode: bool,
) -> (Rect, usize) {
    let body = launch_overlay_chrome_areas(area, debug_mode).body;
    let rows = failure_popup_rows(failure, run_id);
    let rect = failure_popup_rect(body, &rows);
    let state = layout_state(rect, &failure.title, &rows, None, None, None);
    (state.viewport, state.content_height)
}

#[must_use]
pub fn failure_copy_payload(
    failure: &LaunchFailure,
    run_id: &str,
    target: FailureCopyTarget,
) -> Option<String> {
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
    render_dialog_backdrop(frame, chrome.body);
    let rows = failure_popup_rows(failure, run_id);
    let details = detail_rows(&rows);
    let rect = failure_popup_rect(chrome.body, &rows);
    let theme = Theme::default();
    let mut state = DetailTableState {
        hovered: view
            .failure_copy_hover
            .and_then(|target| target_id(&rows, target)),
        copied: view
            .failure_copied
            .and_then(|target| target_id(&rows, target)),
        scroll: view.failure_scroll.clone(),
        ..DetailTableState::default()
    };
    frame.render_stateful_widget(
        &MessageDialog {
            dialog: Dialog {
                title: &failure.title,
                body: Text::from(failure.summary.as_str()),
                style: Style::default(),
                theme: &theme,
                emphasis: PanelEmphasis::Focused,
            },
            details: &details,
            label_width: 0,
            wrap: true,
            theme: &theme,
        },
        rect,
        &mut state,
    );
    if !debug_mode {
        frame.render_widget(Clear, chrome.hint);
    }
    termrock::widgets::render_hint_bar(frame, chrome.hint, &failure_hint_spans());
}

#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "feedback variants are independent product effects"
)]
pub fn failure_popup_hyperlink_overlay(
    area: Rect,
    failure: &LaunchFailure,
    run_id: &str,
    debug_mode: bool,
    scroll: Option<termrock::scroll::DialogScroll>,
    hovered: Option<FailureCopyTarget>,
    copied: Option<FailureCopyTarget>,
    _revealed: Option<FailureCopyTarget>,
    _opened: Option<FailureCopyTarget>,
) -> Vec<u8> {
    let body = launch_overlay_chrome_areas(area, debug_mode).body;
    let rows = failure_popup_rows(failure, run_id);
    let rect = failure_popup_rect(body, &rows);
    let state = layout_state(rect, &failure.title, &rows, scroll, hovered, copied);
    let mut out = Vec::new();
    for region in state.regions {
        let Some(row) = rows.get(region.id) else {
            continue;
        };
        let Some(href) = row.href.as_deref() else {
            continue;
        };
        let visible =
            termrock::display_cols_slice(&row.value, 0, usize::from(region.value_area.width));
        if visible.is_empty() {
            continue;
        }
        out.extend_from_slice(
            format!(
                "\x1b[{};{}H",
                region.value_area.y + 1,
                region.value_area.x + 1
            )
            .as_bytes(),
        );
        out.extend_from_slice(&termrock::osc::encode_hyperlink_open(None, href));
        let color = termrock::LINK_FG;
        out.extend_from_slice(
            format!("\x1b[38;2;{};{};{}m\x1b[1;4m", color.r, color.g, color.b).as_bytes(),
        );
        out.extend_from_slice(visible.as_bytes());
        out.extend_from_slice(&termrock::osc::encode_hyperlink_close());
        out.extend_from_slice(b"\x1b[0m");
    }
    out
}

fn failure_hint_spans() -> Vec<HintSpan<'static>> {
    let mut spans = vec![
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
