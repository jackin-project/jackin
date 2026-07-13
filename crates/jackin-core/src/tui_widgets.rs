// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Stubs for L3 TUI widget types used as test fixtures by
//! `runtime/progress/tests.rs` (and any other L0/L1/L2
//! consumer of `jackin-launch-tui`'s progress fields).
//!
//! Verbatim copies of the L3 types `TailScroll`, `DialogBodyScroll`,
//! and `StatusFooterHover` from `jackin-tui` plus the free
//! helpers `bottom_chrome_areas` and `max_line_width`.
//!
//! Architecture Invariant: depends on `ratatui` (already in
//! the runtime / launch-tui dep graph) for the `Rect` and
//! `Line` types. No `jackin-*` deps.
//!
//! Lifted to L0 per the A5 unblock work: the runtime's
//! progress tests construct these widgets as field values
//! without depending on `jackin-tui`. The L3 copy remains
//! the canonical home for L3 callers; the runtime uses
//! the L0 copy.

use ratatui::layout::Rect;
use ratatui::text::Line;

/// Tail-relative scroll offset helper.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TailScroll {
    offset: usize,
}

impl TailScroll {
    /// Create a tail scroll with the given distance from the bottom.
    #[must_use]
    pub const fn new(offset: usize) -> Self {
        Self { offset }
    }

    /// Distance from the content tail (0 = pinned to bottom).
    #[must_use]
    pub const fn offset(self) -> usize {
        self.offset
    }

    /// Adjust offset by `delta`, clamped to `filled` lines of scroll room.
    pub fn scroll_by(&mut self, filled: usize, delta: isize) -> usize {
        let current = self.offset.min(filled);
        self.offset = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta.unsigned_abs()).min(filled)
        };
        self.offset
    }

    /// Clamp offset so it never exceeds `filled`.
    pub fn clamp(&mut self, filled: usize) -> usize {
        self.offset = self.offset.min(filled);
        self.offset
    }

    /// Convert a tail-relative offset into a top-relative scroll origin.
    #[must_use]
    pub fn to_top_offset(self, content_len: usize, viewport_len: usize) -> usize {
        let max = max_offset(content_len, viewport_len);
        max.saturating_sub(self.offset.min(max))
    }
}

/// Scroll-body state for dialog content with vertical + horizontal axes.
#[derive(Debug, Clone, Default)]
pub struct DialogBodyScroll {
    /// Vertical scroll offset in rows.
    pub scroll_y: u16,
    /// Horizontal scroll offset in columns.
    pub scroll_x: u16,
}

impl DialogBodyScroll {
    /// Zeroed scroll state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            scroll_y: 0,
            scroll_x: 0,
        }
    }
}

/// Status-footer hover state.
#[allow(
    clippy::struct_excessive_bools,
    reason = "Four orthogonal status-footer hover flags (left, usage, right, \
              right_debug) — the L2 bit-field mirror of the L3 \
              `jackin_tui::StatusFooterHover` struct consumed individually by the \
              capsule status-footer renderer. Named-field reads match the per- \
              segment hover-rendering idiom."
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusFooterHover {
    /// Pointer is over the left footer segment.
    pub left: bool,
    /// Pointer is over the usage segment.
    pub usage: bool,
    /// Pointer is over the right footer segment.
    pub right: bool,
    /// Pointer is over the right-debug footer segment.
    pub right_debug: bool,
}

/// Bottom-chrome layout areas (body + hint + spacer + footer rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BottomChromeAreas {
    /// Main body rect above the chrome rows.
    pub body: Rect,
    /// Hint row rect.
    pub hint: Rect,
    /// Spacer row between hint and footer.
    pub spacer: Rect,
    /// Status footer row rect.
    pub footer: Rect,
}

/// Compute the bottom-chrome layout for a given total area.
#[must_use]
pub const fn bottom_chrome_areas(area: Rect) -> BottomChromeAreas {
    BottomChromeAreas {
        body: Rect {
            height: area.height.saturating_sub(BOTTOM_CHROME_ROWS),
            ..area
        },
        hint: row_from_bottom(area, 3),
        spacer: row_from_bottom(area, 2),
        footer: row_from_bottom(area, 1),
    }
}

/// Number of rows reserved for bottom chrome (hint + spacer + footer).
pub const BOTTOM_CHROME_ROWS: u16 = 3;

const fn row_from_bottom(area: Rect, offset: u16) -> Rect {
    Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(offset),
        width: area.width,
        height: if area.height >= offset { 1 } else { 0 },
    }
}

/// Maximum display width among `lines` (0 when empty).
#[must_use]
pub fn max_line_width(lines: &[Line<'_>]) -> usize {
    lines.iter().map(Line::width).max().unwrap_or(0)
}

/// Whether content taller than the viewport can scroll.
#[must_use]
pub const fn is_scrollable(content_len: usize, viewport_len: usize) -> bool {
    viewport_len > 0 && content_len > viewport_len
}

/// Maximum top-relative scroll offset for the given content/viewport sizes.
#[must_use]
pub const fn max_offset(content_len: usize, viewport_len: usize) -> usize {
    if viewport_len == 0 || content_len <= viewport_len {
        0
    } else {
        content_len - viewport_len
    }
}
