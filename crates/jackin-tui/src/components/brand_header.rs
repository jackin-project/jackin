//! Brand header component.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::theme::{BRAND_BLOCK, INK, PHOSPHOR_DARK, WHITE};

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
    // The mark is a green block: black word, white chevron, square edges.
    let block = Style::default()
        .bg(BRAND_BLOCK)
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled(" jackin", block.fg(INK)),
        Span::styled("❯", block.fg(WHITE)),
        Span::styled(" ", block),
        Span::styled(" · ", Style::default().fg(PHOSPHOR_DARK)),
        Span::styled(label.to_owned(), crate::theme::DIM),
    ])
}

pub fn render_brand_header(frame: &mut ratatui::Frame<'_>, area: Rect, label: &str) {
    frame.render_widget(BrandHeader::new(label), area);
}

#[cfg(test)]
mod tests;
