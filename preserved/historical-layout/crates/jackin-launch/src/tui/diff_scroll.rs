// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Diff scroll state of the launch inspect surface.
//!
//! Two offsets, matching the loop this was hoisted from: key input advances
//! one, the renderer clamps and writes back the other.

/// Lines moved by one `PageUp` / `PageDown` press on the inspect surface.
pub const PAGE_STEP: usize = 10;

/// Diff scroll state of the launch inspect surface.
///
/// Two offsets, matching the loop this was hoisted from: key input advances
/// one, the renderer clamps and writes back the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiffScroll {
    key_offset: usize,
    render_offset: usize,
}

impl DiffScroll {
    /// Both offsets start at the top of the diff.
    #[must_use]
    pub fn new() -> Self {
        Self {
            key_offset: 0,
            render_offset: 0,
        }
    }

    /// Offset handed to the widget for one frame, clamped to the diff length.
    #[must_use]
    pub fn offset_for_render(&self, line_count: usize) -> usize {
        self.render_offset.min(line_count.saturating_sub(1))
    }

    /// Stores the offset the widget kept after its own viewport clamp. Key
    /// offset untouched: an over-scrolled key offset survives the render
    /// clamp and later key input resumes from it.
    pub fn record_rendered(&mut self, offset: usize) {
        self.render_offset = offset;
    }

    /// One `Up` / `k` press on the diff pane.
    pub fn line_up(&mut self) {
        self.key_offset = self.key_offset.saturating_sub(1);
        self.render_offset = self.key_offset;
    }

    /// One `Down` / `j` press on the diff pane, clamped to the diff length.
    pub fn line_down(&mut self, line_count: usize) {
        self.key_offset = self
            .key_offset
            .saturating_add(1)
            .min(line_count.saturating_sub(1));
        self.render_offset = self.key_offset;
    }

    /// One `PageUp` press (fires in any pane, exactly as the loop had it).
    pub fn page_up(&mut self) {
        self.key_offset = self.key_offset.saturating_sub(PAGE_STEP);
        self.render_offset = self.key_offset;
    }

    /// One `PageDown` press (fires in any pane), clamped to the diff length.
    pub fn page_down(&mut self, line_count: usize) {
        self.key_offset = self
            .key_offset
            .saturating_add(PAGE_STEP)
            .min(line_count.saturating_sub(1));
        self.render_offset = self.key_offset;
    }

    /// Repo or file selection changed: rebuilt diff starts at the top.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests;
