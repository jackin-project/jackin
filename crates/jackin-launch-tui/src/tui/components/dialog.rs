// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared launch dialog backdrop geometry.

use jackin_tui::components::ModalBackdrop;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::tui::components::chrome::bottom_chrome_areas;

/// Paint the shared solid dialog backdrop over the content body and split the
/// standard bottom chrome into hint/spacer/footer rows.
///
/// Launch's pre-cockpit prompts may leave the footer row blank, while cockpit
/// overlays render their status footer before the dialog and keep it visible.
pub fn dialog_backdrop(frame: &mut Frame<'_>, area: Rect) -> (Rect, Rect) {
    let chrome = bottom_chrome_areas(area);
    frame.render_widget(ModalBackdrop, chrome.body);
    (chrome.body, chrome.hint)
}
