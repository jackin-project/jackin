//! Single-button error dialog component.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};

use crate::ModalOutcome;
use crate::theme::{DANGER_RED, WHITE};

#[derive(Debug, Clone)]
pub struct ErrorPopupState {
    pub title: String,
    pub message: String,
    cached_rows: Cell<Option<(u16, u16)>>,
}

impl ErrorPopupState {
    #[must_use]
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            cached_rows: Cell::new(None),
        }
    }

    #[must_use]
    pub const fn handle_key(&self, key: KeyEvent) -> ModalOutcome<()> {
        match key.code {
            KeyCode::Enter | KeyCode::Char('o' | 'O') | KeyCode::Esc => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }
}

pub struct ErrorDialog<'a> {
    state: &'a ErrorPopupState,
}

impl<'a> ErrorDialog<'a> {
    #[must_use]
    pub const fn new(state: &'a ErrorPopupState) -> Self {
        Self { state }
    }
}

impl Widget for ErrorDialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = format!(" {} ", self.state.title);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(DANGER_RED))
            .title(Span::styled(
                title,
                Style::default().fg(DANGER_RED).add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        Clear.render(area, buf);
        block.render(area, buf);

        let body_rows = inner.height.saturating_sub(3);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(body_rows),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

        Paragraph::new(self.state.message.as_str())
            .style(Style::default().fg(WHITE))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .render(chunks[1], buf);

        let focused_style = Style::default()
            .bg(WHITE)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD);
        Paragraph::new(Line::from(Span::styled("  OK  ", focused_style)))
            .alignment(Alignment::Center)
            .render(chunks[3], buf);
    }
}

#[must_use]
pub fn estimated_message_rows(state: &ErrorPopupState, inner_width: u16) -> u16 {
    if let Some((cached_width, rows)) = state.cached_rows.get()
        && cached_width == inner_width
    {
        return rows;
    }
    let width = usize::from(inner_width.max(1));
    let mut rows: u32 = 0;
    for line in state.message.lines() {
        let len = line.chars().count().max(1);
        rows = rows.saturating_add(u32::try_from(len.div_ceil(width)).unwrap_or(u32::MAX));
    }
    let result = u16::try_from(rows.max(1)).unwrap_or(u16::MAX);
    state.cached_rows.set(Some((inner_width, result)));
    result
}

#[must_use]
pub fn required_height(state: &ErrorPopupState, inner_width: u16, max_rows: u16) -> u16 {
    let body = estimated_message_rows(state, inner_width);
    body.saturating_add(6).min(max_rows.max(7))
}

pub fn render_error_dialog(frame: &mut ratatui::Frame<'_>, area: Rect, state: &ErrorPopupState) {
    frame.render_widget(ErrorDialog::new(state), area);
}

#[cfg(test)]
mod tests;
