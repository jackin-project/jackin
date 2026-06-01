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
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn modal_backdrop_fills_area_with_dialog_backdrop() {
        let backend = TestBackend::new(10, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| frame.render_widget(ModalBackdrop, frame.area()))
            .unwrap();
        let buf = terminal.backend().buffer();
        let expected = crate::theme::color(crate::DIALOG_BACKDROP);
        assert_eq!(buf[(0, 0)].symbol(), " ");
        assert_eq!(buf[(0, 0)].bg, expected);
        assert_eq!(buf[(9, 4)].bg, expected);
    }
}
