pub const LIST_HEADER_HEIGHT: u16 = 2;
pub const LIST_FOOTER_HEIGHT: u16 = 2;
pub const SCREEN_HEADER_HEIGHT: u16 = 3;
pub const TAB_STRIP_HEIGHT: u16 = 2;

/// Like a centered percent-width rect, but takes a fixed row height.
#[must_use]
pub fn centered_rect_fixed(
    outer: ratatui::layout::Rect,
    pct_w: u16,
    rows: u16,
) -> ratatui::layout::Rect {
    let w = outer.width * pct_w / 100;
    let h = rows.min(outer.height);
    ratatui::layout::Rect {
        x: outer.x + outer.width.saturating_sub(w) / 2,
        y: outer.y + outer.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}
