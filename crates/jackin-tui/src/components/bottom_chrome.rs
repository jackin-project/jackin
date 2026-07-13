// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared bottom-chrome row layout.

use ratatui::layout::Rect;

/// Standard status-preserving bottom stack:
///
/// 1. focused-surface hint row
/// 2. blank separator row
/// 3. shared status/footer row
pub const BOTTOM_CHROME_ROWS: u16 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BottomChromeAreas {
    pub body: Rect,
    pub hint: Rect,
    pub spacer: Rect,
    pub footer: Rect,
}

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

const fn row_from_bottom(area: Rect, offset: u16) -> Rect {
    Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(offset),
        width: area.width,
        height: if area.height >= offset { 1 } else { 0 },
    }
}

#[cfg(test)]
mod tests;
