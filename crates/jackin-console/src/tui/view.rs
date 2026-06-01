//! Top-level console frame composition helpers.

use ratatui::{Frame, layout::Rect};

/// How many rows the footer needs to display all `items` within `width`
/// columns. Minimum 1.
#[must_use]
pub fn footer_height(items: &[jackin_tui::HintSpan<'_>], width: u16) -> u16 {
    jackin_tui::components::wrapped_height(items, width)
}

pub fn render_footer(frame: &mut Frame, area: Rect, items: &[jackin_tui::HintSpan<'_>]) {
    jackin_tui::components::render_wrapped_hint_bar(frame, area, items);
}

pub fn render_header(frame: &mut Frame, area: Rect, title: &str) {
    jackin_tui::components::render_brand_header(frame, area, title);
}
