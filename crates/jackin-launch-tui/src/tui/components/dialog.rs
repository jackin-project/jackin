// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared launch dialog backdrop geometry.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::tui::components::chrome::bottom_chrome_areas;

pub fn render_dialog_backdrop(frame: &mut Frame<'_>, area: Rect) {
    let backdrop = termrock::widgets::Backdrop {
        symbol: ' ',
        style: Style::default()
            .fg(Color::Reset)
            .bg(termrock::style::DIALOG_BACKDROP),
    };
    frame.render_widget(&backdrop, area);
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
