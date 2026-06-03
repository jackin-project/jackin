//! Generic non-interactive status popup.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::theme::{PHOSPHOR_GREEN, WHITE};

#[derive(Debug, Clone)]
pub struct StatusPopupState {
    title: String,
    message: String,
}

impl StatusPopupState {
    #[must_use]
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
        }
    }
}

pub fn render_status_popup(frame: &mut Frame<'_>, area: Rect, state: &StatusPopupState) {
    if area.width < 8 || area.height < 5 {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(crate::theme::GREEN)
        .title(Span::styled(
            format!(" {} ", state.title),
            Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(state.message.as_str())
            .style(Style::default().fg(WHITE))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new("Please wait")
            .style(crate::theme::DIM)
            .alignment(Alignment::Center),
        chunks[2],
    );
}
