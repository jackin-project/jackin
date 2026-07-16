//! jackin❯ brand header composition.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

#[derive(Debug, Clone, Copy)]
struct BrandHeader<'a> {
    label: &'a str,
}

impl Widget for BrandHeader<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        Paragraph::new(brand_header_line(self.label))
            .alignment(Alignment::Left)
            .render(area, buffer);
    }
}

fn brand_header_line(label: &str) -> Line<'static> {
    let block = Style::default()
        .bg(termrock::style::BRAND_BLOCK)
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled(" jackin", block.fg(termrock::style::INK)),
        Span::styled("❯", block.fg(termrock::style::WHITE)),
        Span::styled(" ", block),
        Span::styled(" · ", Style::default().fg(termrock::style::PHOSPHOR_DARK)),
        Span::styled(label.to_owned(), termrock::style::DIM),
    ])
}

pub fn render_brand_header(frame: &mut ratatui::Frame<'_>, area: Rect, label: &str) {
    frame.render_widget(BrandHeader { label }, area);
}
