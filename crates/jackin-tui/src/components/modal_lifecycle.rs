// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Modal input/lifecycle helper — backdrop, click-outside-dismiss, one-bright-border.
//!
//! Any surface that hosts a modal dialog uses this helper to:
//! 1. Render the full-screen opaque backdrop before drawing the modal.
//! 2. Determine whether a mouse click dismisses the modal (click outside) or
//!    is swallowed (click inside with no interactive target).
//!
//! This centralises the three behaviors that previously had to be
//! re-implemented per surface (console, launch, capsule).

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::components::ModalBackdrop;

/// Render the full-screen opaque backdrop. Call this before drawing the modal
/// so everything behind it (other dialogs, main UI) is hidden.
pub fn render_backdrop(frame: &mut Frame<'_>, full_area: Rect) {
    frame.render_widget(ModalBackdrop, full_area);
}

/// Classify a mouse click relative to an open modal rect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalClickResult {
    /// Click was outside the modal rect — dismiss the modal (same as Esc).
    OutsideDismiss,
    /// Click was inside the modal rect on the given col/row — let the modal handle it.
    InsideHit,
}

/// Classify a click at `(col, row)` against `modal_rect`.
///
/// Returns:
/// - `OutsideDismiss` if the click is outside the modal (dismiss the modal).
/// - `InsideHit` if inside (caller decides what to do within the modal).
#[must_use]
pub fn classify_click(modal_rect: Rect, col: u16, row: u16) -> ModalClickResult {
    if modal_rect.contains(ratatui::layout::Position { x: col, y: row }) {
        ModalClickResult::InsideHit
    } else {
        ModalClickResult::OutsideDismiss
    }
}

#[cfg(test)]
mod tests;
