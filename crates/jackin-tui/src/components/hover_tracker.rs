// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared hover-affordance helper.
//!
//! `HoverTracker` centralises the per-frame "which rect is the pointer over?"
//! computation. Surfaces register their clickable rects each frame, then call
//! `update()` once with the current mouse position to learn which target (if any)
//! the pointer is over.
//!
//! This removes the scattered `hovered_tab`, `hovered_mount_row`, `menu_hovered`
//! booleans and gives each surface a single code path for both OSC 22 pointer
//! shape and hover-colour styling.

use ratatui::layout::Rect;

/// A clickable element identified by an application-defined key `K`.
#[derive(Debug, Clone)]
pub(crate) struct ClickableRect<K> {
    pub(crate) rect: Rect,
    pub(crate) key: K,
}

/// Per-frame hover tracker: register clickable rects, then query the hovered target.
///
/// # Usage
///
/// 1. Clear the tracker at the start of each mouse-event handler.
/// 2. Register every clickable element by calling `register()`.
/// 3. Call `hovered(col, row)` to get the current hover target.
/// 4. Pass the result to `style_for(key, hovered)` to get the correct style.
#[derive(Debug, Clone, Default)]
pub struct HoverTracker<K: Clone + PartialEq> {
    entries: Vec<ClickableRect<K>>,
}

impl<K: Clone + PartialEq> HoverTracker<K> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Clear all registered rects (call at the start of each event-handling cycle).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Register a clickable rect with its identifier.
    pub fn register(&mut self, rect: Rect, key: K) {
        self.entries.push(ClickableRect { rect, key });
    }

    /// Return the key of the element the pointer is over, or `None`.
    ///
    /// `col` and `row` are 0-based terminal coordinates.
    #[must_use]
    pub fn hovered(&self, col: u16, row: u16) -> Option<&K> {
        self.entries.iter().find_map(|entry| {
            if entry
                .rect
                .contains(ratatui::layout::Position { x: col, y: row })
            {
                Some(&entry.key)
            } else {
                None
            }
        })
    }

    /// Return `true` when the pointer is over the element identified by `key`.
    #[must_use]
    pub fn is_hovered(&self, col: u16, row: u16, key: &K) -> bool {
        self.hovered(col, row).is_some_and(|k| k == key)
    }

    /// Return the hover style for an element: lifted (hover) or resting.
    ///
    /// Callers provide both styles; the tracker picks based on whether the
    /// element is currently hovered. This keeps the tracker backend-agnostic.
    #[must_use]
    pub fn pick_style<S: Clone>(&self, col: u16, row: u16, key: &K, hover: S, resting: S) -> S {
        if self.is_hovered(col, row, key) {
            hover
        } else {
            resting
        }
    }

    /// Return whether any registered element is currently hovered.
    ///
    /// Feed the result into [`crate::clickable_pointer_shape`] and
    /// [`crate::osc22_pointer_shape`] to toggle the OSC 22 pointer cue.
    #[must_use]
    pub fn any_hovered(&self, col: u16, row: u16) -> bool {
        self.hovered(col, row).is_some()
    }
}

#[cfg(test)]
mod tests;
