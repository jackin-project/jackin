use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use termrock::text::LinePlacement;

#[derive(Debug, Clone, Copy)]
struct BrandHeader<'a> {
    label: &'a str,
}

impl Widget for BrandHeader<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let mut scratch = String::new();
        termrock::text::paint_line_overflow(
            buffer,
            area,
            &brand_header_line(self.label),
            Style::default(),
            LinePlacement::clipped("…"),
            &mut scratch,
        );
    }
}

fn brand_header_line(label: &str) -> Line<'static> {
    let block = Style::default()
        .bg(jackin_tui::tokens::BRAND_BLOCK)
        .add_modifier(Modifier::BOLD);

    Line::from(vec![
        Span::styled(" jackin", block.fg(jackin_tui::tokens::INK)),
        Span::styled("❯", block.fg(jackin_tui::tokens::BRAND_CHEVRON)),
        Span::styled(" ", block),
        Span::styled(
            " · ",
            Style::default().fg(jackin_tui::tokens::BRAND_SEPARATOR),
        ),
        Span::styled(
            label.to_owned(),
            Style::default().fg(jackin_tui::tokens::BRAND_LABEL),
        ),
    ])
}

pub fn render_brand_header(frame: &mut ratatui::Frame<'_>, area: Rect, label: &str) {
    frame.render_widget(BrandHeader { label }, area);
}

#[cfg(test)]
mod tests;
