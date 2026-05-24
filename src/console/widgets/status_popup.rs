use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::{PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

#[derive(Debug, Clone)]
pub struct StatusPopupState {
    title: String,
    message: String,
}

impl StatusPopupState {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &StatusPopupState) {
    // Skip render when the slot is too small for the border + at least
    // one message row + one footer row. Avoids ratatui collapsing the
    // `Min(1)` arm to zero on tiny terminals and stomping the border.
    if area.width < 8 || area.height < 5 {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_GREEN))
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
            .style(Style::default().fg(PHOSPHOR_DIM))
            .alignment(Alignment::Center),
        chunks[2],
    );
}
