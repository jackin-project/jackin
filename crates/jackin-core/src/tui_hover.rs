// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Incremental hover registration on TermRock [`HitRegion`] geometry.
//!
//! Surfaces that assemble hit geometry over the course of a frame register
//! regions here, then query pointer identity. Geometry uses TermRock's
//! canonical [`HitRegion`] type; when a full painted region slice is already
//! available prefer [`termrock::interaction::HoverState`] directly.

use ratatui::layout::{Position, Rect};
use termrock::interaction::HitRegion;

/// Incremental hover registry used by console and capsule chrome hit-testing.
#[derive(Debug, Clone, Default)]
pub struct HoverTracker<K: Clone + PartialEq> {
    regions: Vec<HitRegion<K>>,
}

impl<K: Clone + PartialEq> HoverTracker<K> {
    /// Create an empty hover registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            regions: Vec::new(),
        }
    }

    /// Drop every registered hit rectangle.
    pub fn clear(&mut self) {
        self.regions.clear();
    }

    /// Register a painted rectangle and its product identity.
    pub fn register(&mut self, rect: Rect, key: K) {
        self.regions.push(HitRegion {
            id: key,
            area: rect,
        });
    }

    /// Borrow the registered regions for a TermRock [`HoverState`] update.
    #[must_use]
    pub fn regions(&self) -> &[HitRegion<K>] {
        &self.regions
    }

    /// Return the identity under `(col, row)`, if any.
    #[must_use]
    pub fn hovered(&self, col: u16, row: u16) -> Option<&K> {
        let position = Position { x: col, y: row };
        self.regions
            .iter()
            .find(|region| region.area.contains(position))
            .map(|region| &region.id)
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
