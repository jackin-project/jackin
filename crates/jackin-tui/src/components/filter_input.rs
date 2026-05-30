//! Canonical single-row filter input component.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::theme::{PHOSPHOR_DARK, PHOSPHOR_DIM, WHITE};

#[derive(Debug, Clone, Copy)]
pub struct FilterInput<'a> {
    filter: &'a str,
}

impl<'a> FilterInput<'a> {
    #[must_use]
    pub const fn new(filter: &'a str) -> Self {
        Self { filter }
    }
}

impl Widget for FilterInput<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(filter_input_line(self.filter)).render(area, buf);
    }
}

#[must_use]
pub fn filter_input_line(filter: &str) -> Line<'static> {
    if filter.is_empty() {
        Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(PHOSPHOR_DIM)),
            Span::styled("\u{2591}".repeat(20), Style::default().fg(PHOSPHOR_DARK)),
        ])
    } else {
        Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(PHOSPHOR_DIM)),
            Span::styled(filter.to_string(), Style::default().fg(WHITE)),
            Span::styled(
                "\u{2588}",
                Style::default()
                    .fg(WHITE)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ])
    }
}

pub fn render_filter_input(frame: &mut ratatui::Frame<'_>, area: Rect, filter: &str) {
    frame.render_widget(FilterInput::new(filter), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn empty_filter_shows_placeholder() {
        let backend = TestBackend::new(32, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| frame.render_widget(FilterInput::new(""), frame.area()))
            .unwrap();
        let row: String = (0..32)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_string())
            .collect();
        assert!(row.contains("Filter: ░░░░░░░░░░░░░░░░░░░░"));
    }

    #[test]
    fn populated_filter_shows_cursor() {
        let line = filter_input_line("abc");
        let joined: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(joined, "Filter: abc█");
        assert!(
            line.spans[2]
                .style
                .add_modifier
                .contains(Modifier::SLOW_BLINK)
        );
    }
}
