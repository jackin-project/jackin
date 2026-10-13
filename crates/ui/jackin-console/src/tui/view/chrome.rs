// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Footer, header, and confirm-area rendering.

use ratatui::{Frame, layout::Rect};

#[must_use]
pub const fn workspace_header_title() -> &'static str {
    "workspaces"
}

/// How many rows the footer needs to display all `items` within `width`
/// columns. Includes one leading blank spacer row above the hints.
#[must_use]
pub fn footer_height(items: &[termrock::widgets::HintSpan<'_>], width: u16) -> u16 {
    // +1 for the mandatory leading spacer row above the hints on every screen.
    u16::try_from(
        termrock::widgets::wrapped_hint_lines(
            items,
            width,
            &termrock::style::DesignSystem::default(),
        )
        .len(),
    )
    .unwrap_or(u16::MAX)
    .saturating_add(1)
}

#[must_use]
pub const fn effective_footer_height(height: u16) -> u16 {
    if height == 0 { 1 } else { height }
}

#[must_use]
pub fn measured_footer_height(items: &[termrock::widgets::HintSpan<'_>], width: u16) -> u16 {
    effective_footer_height(footer_height(items, width))
}

pub fn render_footer(frame: &mut Frame<'_>, area: Rect, items: &[termrock::widgets::HintSpan<'_>]) {
    if area.height == 0 {
        return;
    }
    // Render hints in the bottom portion; the top row is the leading spacer.
    let hint_rows = area.height.saturating_sub(1).max(1);
    let hint_area = Rect {
        x: area.x,
        y: area.y.saturating_add(area.height.saturating_sub(hint_rows)),
        width: area.width,
        height: hint_rows,
    };
    frame.render_widget(
        ratatui::widgets::Paragraph::new(termrock::widgets::wrapped_hint_lines(
            items,
            hint_area.width,
            &termrock::style::DesignSystem::default(),
        ))
        .alignment(ratatui::layout::Alignment::Center),
        hint_area,
    );
}

pub fn render_header(frame: &mut Frame<'_>, area: Rect, title: &str) {
    crate::tui::components::brand_header::render_brand_header(frame, area, title);
}

pub fn render_modal_backdrop(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(termrock::widgets::Backdrop::default(), area);
}

#[must_use]
pub fn delete_confirm_area(area: Rect) -> Rect {
    // Stage-level confirm (not a `ConsoleModal`): wraps shared centering
    // directly, as it did beside the retired rect registry.
    crate::tui::layout::centered_rect_fixed(area, 60, 7)
}

#[must_use]
pub fn purge_confirm_area(area: Rect) -> Rect {
    // Stage-level confirm (not a `ConsoleModal`): wraps shared centering
    // directly, as it did beside the retired rect registry.
    crate::tui::layout::centered_rect_fixed(area, 70, 9)
}

#[must_use]
pub fn settings_error_area(area: Rect, height: u16) -> Rect {
    // Structural exception: legacy console status/error helpers wrap shared centering while callers supply footer-excluded areas.
    crate::tui::layout::centered_rect_fixed(area, 60, height)
}

#[must_use]
pub fn status_overlay_area(area: Rect) -> Rect {
    crate::tui::layout::centered_rect_fixed(area, 50, 7)
}
