//! Transient non-blocking overlay toast.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::display_cols;
use crate::theme::{PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE};

#[derive(Debug, Clone, Copy)]
pub struct Toast<'a> {
    message: &'a str,
    right_margin: u16,
    top_margin: u16,
}

impl<'a> Toast<'a> {
    #[must_use]
    pub const fn new(message: &'a str) -> Self {
        Self {
            message,
            right_margin: 2,
            top_margin: 1,
        }
    }

    #[must_use]
    pub const fn right_margin(mut self, margin: u16) -> Self {
        self.right_margin = margin;
        self
    }

    #[must_use]
    pub const fn top_margin(mut self, rows: u16) -> Self {
        self.top_margin = rows;
        self
    }
}

#[must_use]
pub fn toast_rect(area: Rect, toast: Toast<'_>) -> Option<Rect> {
    if area.width == 0 || area.height == 0 || toast.message.is_empty() {
        return None;
    }

    let desired_width = u16::try_from(display_cols(toast.message) + 4).unwrap_or(u16::MAX);
    let width = desired_width.min(area.width);
    let height = 3.min(area.height);
    let right_edge = area.right().saturating_sub(toast.right_margin);
    let x = right_edge.saturating_sub(width).max(area.left());
    let y = area
        .top()
        .saturating_add(toast.top_margin)
        .min(area.bottom().saturating_sub(height).max(area.top()));

    Some(Rect {
        x,
        y,
        width,
        height,
    })
}

pub fn render_toast(frame: &mut Frame<'_>, area: Rect, toast: Toast<'_>) {
    let Some(area) = toast_rect(area, toast) else {
        return;
    };
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_GREEN))
        .style(Style::default().bg(PHOSPHOR_DARK));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Span::styled(
            toast.message,
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        )),
        inner,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    use super::*;

    #[test]
    fn toast_rect_anchors_to_top_right() {
        let rect = toast_rect(
            Rect::new(0, 0, 149, 39),
            Toast::new("Selection copied").top_margin(1),
        )
        .expect("toast should fit");

        assert_eq!(rect.height, 3);
        assert_eq!(rect.width, 20);
        assert_eq!(rect.x, 127);
        assert_eq!(rect.y, 1);
    }

    #[test]
    fn render_toast_draws_message_and_border() {
        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_toast(
                    frame,
                    frame.area(),
                    Toast::new("Selection copied").top_margin(1),
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let rendered = format!("{buffer:?}");
        assert!(rendered.contains("Selection copied"));
        assert_eq!(buffer[(18, 1)].fg, PHOSPHOR_GREEN);
    }
}
