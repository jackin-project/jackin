//! Launch-specific bottom chrome layout.

use ratatui::layout::Rect;

pub const BOTTOM_CHROME_ROWS: u16 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BottomChromeAreas {
    pub body: Rect,
    pub hint: Rect,
    pub spacer: Rect,
    pub footer: Rect,
}

#[must_use]
pub fn bottom_chrome_areas(area: Rect) -> BottomChromeAreas {
    let (body, [hint, spacer, footer]) = termrock::layout::bottom_rows(area, [1, 1, 1]);
    BottomChromeAreas {
        body,
        hint,
        spacer,
        footer,
    }
}
