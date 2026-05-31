//! Shared launch dialog backdrop geometry.

use jackin_tui::theme::DIALOG_BACKDROP;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;

/// Paint the shared solid dialog backdrop over `area` (capsule modal
/// convention — hide the cockpit, never dim it) and split off the bottom row
/// for the footer hint. Returns `(box_area, hint_area)` so every launch dialog
/// centers its box and renders its hint the same way.
pub fn dialog_backdrop(frame: &mut Frame<'_>, area: Rect) -> (Rect, Rect) {
    frame.render_widget(
        Block::default().style(Style::default().bg(DIALOG_BACKDROP)),
        area,
    );
    let box_area = Rect {
        height: area.height.saturating_sub(1),
        ..area
    };
    let hint_area = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: 1,
        ..area
    };
    (box_area, hint_area)
}
