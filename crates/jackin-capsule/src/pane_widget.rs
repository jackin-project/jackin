//! Custom pane-body widget for rendering a vt100 screen into a Ratatui Buffer.
//!
//! This is the implementation of the custom-cell-widget approach chosen in
//! ADR-004. It blits `vt100::Screen` cells directly into the Ratatui `Buffer`
//! so the existing `Buffer::diff` mechanism in the `SocketBackend` handles the
//! actual terminal output — no hand-rolled row diff needed.
//!
//! **Why not tui-term?** See ADR-004: tui-term 0.3.4 implements its `Screen`
//! trait for the crates.io `vt100::Screen`, which is incompatible at the type
//! level with the `donbeave/vt100-rust` fork this codebase requires.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
    widgets::Widget,
};

/// A Ratatui widget that renders a `vt100::Screen` into the given area.
///
/// Each vt100 cell is mapped to the corresponding `ratatui::buffer::Cell`.
/// The Ratatui double-buffer diff in `SocketBackend` then only emits the
/// cells that changed since the last frame, replacing the hand-rolled
/// `PaneBodyCache` row-diff path.
pub struct PaneBodyWidget<'a> {
    screen: &'a vt100::Screen,
}

impl<'a> PaneBodyWidget<'a> {
    #[must_use]
    pub const fn new(screen: &'a vt100::Screen) -> Self {
        Self { screen }
    }
}

impl Widget for PaneBodyWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (screen_rows, screen_cols) = self.screen.size();
        for row in 0..area.height.min(screen_rows) {
            for col in 0..area.width.min(screen_cols) {
                let Some(cell) = self.screen.cell(row, col) else {
                    continue;
                };
                let buf_cell = &mut buf[(area.x + col, area.y + row)];

                let contents = cell.contents();
                if !contents.is_empty() {
                    buf_cell.set_symbol(contents);
                } else {
                    buf_cell.set_char(' ');
                }

                buf_cell.set_fg(vt100_color(cell.fgcolor()));
                buf_cell.set_bg(vt100_color(cell.bgcolor()));

                let mut modifier = Modifier::empty();
                if cell.bold() {
                    modifier |= Modifier::BOLD;
                }
                if cell.italic() {
                    modifier |= Modifier::ITALIC;
                }
                if cell.underline() {
                    modifier |= Modifier::UNDERLINED;
                }
                if cell.inverse() {
                    modifier |= Modifier::REVERSED;
                }
                buf_cell.modifier = modifier;
            }
        }
    }
}

fn vt100_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(idx) => Color::Indexed(idx),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn pane_widget_renders_text_into_buffer() {
        let mut parser = vt100::Parser::new(5, 20, 100);
        parser.process(b"hello world");
        let screen = parser.screen().clone();

        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(PaneBodyWidget::new(&screen), frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let row0: String = (0..20).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(row0.starts_with("hello world"), "expected text in buffer: {row0:?}");
    }

    #[test]
    fn pane_widget_maps_color_reset() {
        let color = vt100_color(vt100::Color::Default);
        assert_eq!(color, Color::Reset);
    }

    #[test]
    fn pane_widget_maps_indexed_color() {
        let color = vt100_color(vt100::Color::Idx(196));
        assert_eq!(color, Color::Indexed(196));
    }

    #[test]
    fn pane_widget_maps_rgb_color() {
        let color = vt100_color(vt100::Color::Rgb(0, 255, 65));
        assert_eq!(color, Color::Rgb(0, 255, 65));
    }
}
