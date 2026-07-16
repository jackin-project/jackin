// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared launch dialog backdrop geometry.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::tui::components::chrome::bottom_chrome_areas;

pub fn render_dialog_backdrop(frame: &mut Frame<'_>, area: Rect) {
    let backdrop = termrock::widgets::Backdrop::new().style(
        Style::default()
            .fg(Color::Reset)
            .bg(jackin_core::tui_theme::DIALOG_BACKDROP),
    );
    frame.render_widget(backdrop, area);
}

#[must_use]
pub fn exact_dialog_rect(area: Rect, width: u16, height: u16) -> Rect {
    termrock::layout::resolve_dialog(
        area,
        termrock::layout::DialogSpec {
            min_width: width,
            preferred_width: width,
            max_width: width,
            min_height: height,
            preferred_height: height,
            max_height: height,
            horizontal_margin: 0,
            vertical_margin: 0,
            placement: termrock::layout::Placement::Centered,
        },
    )
}

#[must_use]
pub fn percent_dialog_rect(
    area: Rect,
    width_pct: u16,
    min_width: u16,
    width_margin: u16,
    height_margin: u16,
    height: u16,
) -> Rect {
    let max_width = area.width.saturating_sub(width_margin).max(min_width);
    let width = (area.width.saturating_mul(width_pct) / 100).clamp(min_width, max_width);
    exact_dialog_rect(
        area,
        width,
        height.min(area.height.saturating_sub(height_margin)),
    )
}

#[must_use]
pub fn dialog_scroll_axes(
    content_width: usize,
    content_height: usize,
    rect: Rect,
) -> termrock::scroll::ScrollAxes {
    termrock::scroll::ScrollAxes {
        vertical: termrock::scroll::is_scrollable(
            content_height,
            usize::from(rect.height.saturating_sub(2)),
        ),
        horizontal: termrock::scroll::is_scrollable(
            content_width,
            usize::from(rect.width.saturating_sub(2)),
        ),
    }
}

pub fn dialog_scroll(scroll: &termrock::scroll::DialogScroll) -> termrock::scroll::DialogScroll {
    let mut copy = termrock::scroll::DialogScroll::default();
    copy.scroll_x = scroll.scroll_x;
    copy.scroll_y = scroll.scroll_y;
    copy
}

/// Paint the shared solid dialog backdrop over the content body and split the
/// standard bottom chrome into hint/spacer/footer rows.
///
/// Launch's pre-cockpit prompts may leave the footer row blank, while cockpit
/// overlays render their status footer before the dialog and keep it visible.
pub fn dialog_backdrop(frame: &mut Frame<'_>, area: Rect) -> (Rect, Rect) {
    let chrome = bottom_chrome_areas(area);
    render_dialog_backdrop(frame, chrome.body);
    (chrome.body, chrome.hint)
}
