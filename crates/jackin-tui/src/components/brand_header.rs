//! Brand header component.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::theme::{PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN};

#[derive(Debug, Clone, Copy)]
pub struct BrandHeader<'a> {
    label: &'a str,
}

impl<'a> BrandHeader<'a> {
    #[must_use]
    pub const fn new(label: &'a str) -> Self {
        Self { label }
    }
}

impl Widget for BrandHeader<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(brand_header_line(self.label))
            .alignment(Alignment::Left)
            .render(area, buf);
    }
}

#[must_use]
pub fn brand_header_line(label: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            " jackin' ",
            Style::default()
                .bg(PHOSPHOR_GREEN)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", Style::default().fg(PHOSPHOR_DARK)),
        Span::styled(label.to_string(), Style::default().fg(PHOSPHOR_DIM)),
    ])
}

pub fn render_brand_header(frame: &mut ratatui::Frame<'_>, area: Rect, label: &str) {
    frame.render_widget(BrandHeader::new(label), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn renders_brand_pill_and_label() {
        let backend = TestBackend::new(32, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| frame.render_widget(BrandHeader::new("Console"), frame.area()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let row: String = (0..32)
            .map(|x| buffer[(x, 0)].symbol().to_string())
            .collect();
        assert!(row.contains(" jackin'  · Console"), "row: {row:?}");
        assert_eq!(buffer[(1, 0)].bg, PHOSPHOR_GREEN);
        assert_eq!(buffer[(11, 0)].fg, PHOSPHOR_DARK);
    }
}
