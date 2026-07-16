// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Product-owned hover registry for surfaces that assemble hit geometry
//! incrementally before querying pointer identity.
//!
//! TermRock's [`termrock::interaction::HoverState`] is the preferred API when
//! painted [`termrock::interaction::HitRegion`]s are already available. This
//! registry only exists for jackin❯ paths that still register rects while
//! building chrome geometry.

use ratatui::layout::{Position, Rect};

#[derive(Debug, Clone)]
struct Entry<K> {
    rect: Rect,
    key: K,
}

/// Incremental hover registry used by console and capsule chrome hit-testing.
#[derive(Debug, Clone, Default)]
pub struct HoverTracker<K: Clone + PartialEq> {
    entries: Vec<Entry<K>>,
}

impl<K: Clone + PartialEq> HoverTracker<K> {
    /// Create an empty hover registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Drop every registered hit rectangle.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Register a painted rectangle and its product identity.
    pub fn register(&mut self, rect: Rect, key: K) {
        self.entries.push(Entry { rect, key });
    }

    /// Return the identity under `(col, row)`, if any.
    #[must_use]
    pub fn hovered(&self, col: u16, row: u16) -> Option<&K> {
        let position = Position { x: col, y: row };
        self.entries
            .iter()
            .find(|entry| entry.rect.contains(position))
            .map(|entry| &entry.key)
    }

    /// Return whether `key` is currently under the pointer.
    #[must_use]
    pub fn is_hovered(&self, col: u16, row: u16, key: &K) -> bool {
        self.hovered(col, row).is_some_and(|k| k == key)
    }

    /// Return whether any registered rectangle contains the pointer.
    #[must_use]
    pub fn any_hovered(&self, col: u16, row: u16) -> bool {
        self.hovered(col, row).is_some()
    }
}
