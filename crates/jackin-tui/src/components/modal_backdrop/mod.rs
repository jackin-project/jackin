//! Shared modal backdrop.

use ratatui::{
    buffer::Buffer,
    style::{Color, Modifier},
    widgets::Widget,
};

/// Fills the target area with the canonical dialog backdrop.
pub struct ModalBackdrop;

impl Widget for ModalBackdrop {
    fn render(self, area: ratatui::layout::Rect, buf: &mut Buffer) {
        let bg = crate::theme::color(crate::DIALOG_BACKDROP);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let cell = &mut buf[(x, y)];
                cell.set_char(' ');
                cell.set_bg(bg);
                cell.set_fg(Color::Reset);
                cell.modifier = Modifier::empty();
            }
        }
    }
}

#[cfg(test)]
mod tests;
