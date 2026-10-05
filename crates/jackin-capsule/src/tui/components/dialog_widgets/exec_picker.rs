// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Approval rendering shares one exact invocation with execution.

use crate::exec::ExecPickerState;
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Clear, Paragraph, Widget},
};
use termrock::{
    style::{DesignSystem, SelectionChrome},
    widgets::{List, ListRow, ListState, Panel, PanelChrome},
};

/// Hard-wrap without trimming whitespace or losing argument characters.
fn argv_lines(state: &ExecPickerState, width: u16) -> Vec<String> {
    let width = usize::from(width).max(1);
    let mut rows = Vec::new();
    let mut row = String::new();
    let mut cols = 0;
    for ch in state.invocation.approval_argv().chars() {
        let ch_cols = termrock::text::display_cols(&ch.to_string());
        if cols + ch_cols > width && !row.is_empty() {
            rows.push(std::mem::take(&mut row));
            cols = 0;
        }
        row.push(ch);
        cols += ch_cols;
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

pub(crate) fn required_height(state: &ExecPickerState, width: u16) -> u16 {
    u16::try_from(
        argv_lines(state, width.saturating_sub(2))
            .len()
            .saturating_add(state.items.len())
            .saturating_add(3),
    )
    .unwrap_or(u16::MAX)
}

fn body_layout(inner: Rect, item_count: usize) -> (Rect, Rect, Rect) {
    let indicator_height = u16::from(inner.height >= 2);
    let available = inner.height.saturating_sub(indicator_height);
    // Keep at least one argv row in every nonempty viewport. The credential
    // list has its own selected-row scrolling, independently of argv paging.
    let credential_height = u16::try_from(item_count)
        .unwrap_or(u16::MAX)
        .min(available / 2);
    let argv_height = available.saturating_sub(credential_height);
    (
        Rect::new(inner.x, inner.y, inner.width, argv_height),
        Rect::new(
            inner.x,
            inner.y.saturating_add(argv_height),
            inner.width,
            indicator_height,
        ),
        Rect::new(
            inner.x,
            inner
                .y
                .saturating_add(argv_height)
                .saturating_add(indicator_height),
            inner.width,
            credential_height,
        ),
    )
}

pub(crate) fn render(frame: &mut Frame<'_>, area: Rect, state: &ExecPickerState) {
    let theme = DesignSystem::default();
    let panel = Panel::new(&theme)
        .title("Attach credentials · exact argv")
        .emphasis(PanelChrome::Focused);
    let inner = panel.inner(area);
    Clear.render(area, frame.buffer_mut());
    frame.render_widget(&panel, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let (argv_area, indicator_area, credential_area) = body_layout(inner, state.items.len());
    let lines = argv_lines(state, argv_area.width);
    let max_scroll = lines.len().saturating_sub(usize::from(argv_area.height));
    let offset = state
        .argv_scroll
        .load(std::sync::atomic::Ordering::Relaxed)
        .min(max_scroll);
    // Snapshots share only the view offset, so normalization after resize also
    // reaches the next key event; invocation data stays immutable.
    state
        .argv_scroll
        .store(offset, std::sync::atomic::Ordering::Relaxed);
    let visible = lines
        .iter()
        .skip(offset)
        .take(usize::from(argv_area.height))
        .cloned()
        .map(Line::from)
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(visible), argv_area);
    // The direction comes first so even a one-column indicator shows that
    // argv continues beyond this viewport.
    let more = match (offset > 0, offset < max_scroll) {
        (true, true) => "↕ more argv · ",
        (true, false) => "↑ more argv · ",
        (false, true) => "↓ more argv · ",
        (false, false) => "",
    };
    frame.render_widget(
        Paragraph::new(format!(
            "{more}argv {}/{} · PgUp/PgDn",
            offset + 1,
            lines.len()
        )),
        indicator_area,
    );
    // A one-row body keeps its sole row for inspectable argv. Its overflow
    // marker occupies the top border instead of hiding any command character.
    if indicator_area.height == 0 && !more.is_empty() {
        frame.render_widget(
            Paragraph::new(more.chars().next().unwrap().to_string()),
            Rect::new(inner.x, area.y, 1, 1),
        );
    }
    if credential_area.height == 0 {
        return;
    }
    let rows = state
        .items
        .iter()
        .enumerate()
        .map(|(id, item)| {
            let mark = if item.selected { "[x]" } else { "[ ]" };
            ListRow::item(
                id,
                Line::from(format!("{mark} {}  {}", item.binding.name, item.display)),
            )
        })
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        &List::new(&rows, &theme.selection(SelectionChrome::Marker)),
        credential_area,
        &mut ListState::new(Some(state.cursor)),
    );
}

#[cfg(test)]
mod tests;
