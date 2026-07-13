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

/// Stack of modal dialogs with "Esc walks back one step" semantics.
///
/// The active modal lives in `current`; every sub-modal push moves the previous
/// active modal into `parents`. `pop` restores exactly one parent, while
/// `clear_chain` closes the whole flow after a terminal commit/cancel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModalStack<M> {
    current: Option<M>,
    parents: Vec<M>,
}

impl<M> Default for ModalStack<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> ModalStack<M> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            current: None,
            parents: Vec::new(),
        }
    }

    #[must_use]
    pub fn from_current(current: M) -> Self {
        Self {
            current: Some(current),
            parents: Vec::new(),
        }
    }

    #[must_use]
    pub fn from_parts(current: Option<M>, parents: Vec<M>) -> Self {
        Self { current, parents }
    }

    #[must_use]
    pub fn into_parts(self) -> (Option<M>, Vec<M>) {
        (self.current, self.parents)
    }

    #[must_use]
    pub const fn current(&self) -> Option<&M> {
        self.current.as_ref()
    }

    #[must_use]
    pub fn current_mut(&mut self) -> Option<&mut M> {
        self.current.as_mut()
    }

    #[must_use]
    pub fn parents(&self) -> &[M] {
        &self.parents
    }

    #[must_use]
    pub fn parents_mut(&mut self) -> &mut Vec<M> {
        &mut self.parents
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.current.is_some()
    }

    #[must_use]
    pub fn has_parent(&self) -> bool {
        !self.parents.is_empty()
    }

    #[must_use]
    pub fn depth(&self) -> usize {
        self.parents.len() + usize::from(self.current.is_some())
    }

    /// Open a new root modal and discard any existing parent chain.
    pub fn open(&mut self, modal: M) {
        self.current = Some(modal);
        self.parents.clear();
    }

    /// Open a child modal, preserving the existing active modal as the parent.
    pub fn open_sub(&mut self, child: M) {
        if let Some(parent) = self.current.take() {
            self.parents.push(parent);
        }
        self.current = Some(child);
    }

    /// Close the active modal and restore one parent, if any.
    pub fn pop(&mut self) {
        self.current = self.parents.pop();
    }

    /// Dismiss only the active modal, leaving parents intact for callers that
    /// intentionally manage parent restoration themselves.
    pub fn dismiss_current(&mut self) {
        self.current = None;
    }

    /// Close the active modal and every saved parent.
    pub fn clear_chain(&mut self) {
        self.current = None;
        self.parents.clear();
    }

    pub fn take_current(&mut self) -> Option<M> {
        self.current.take()
    }
}

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
