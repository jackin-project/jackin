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
        .bg(jackin_tui::tokens::BRAND_BLOCK)
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled(" jackin", block.fg(jackin_tui::tokens::INK)),
        Span::styled(
            "❯",
            block.fg(termrock::Theme::default()
                .style(termrock::style::Role::Text)
                .fg
                .unwrap_or_default()),
        ),
        Span::styled(" ", block),
        Span::styled(
            " · ",
            Style::default().fg(termrock::Theme::default()
                .style(termrock::style::Role::ScrollTrack)
                .fg
                .unwrap_or_default()),
        ),
        Span::styled(
            label.to_owned(),
            termrock::Theme::default().style(termrock::style::Role::TextMuted),
        ),
    ])
}

pub fn render_brand_header(frame: &mut ratatui::Frame<'_>, area: Rect, label: &str) {
    frame.render_widget(BrandHeader { label }, area);
}
