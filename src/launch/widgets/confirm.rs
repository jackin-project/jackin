//! Y/N confirmation modal. Centered, bordered, two-line body.
//! Y / N / Esc return distinct outcomes; case-insensitive.

use crossterm::event::{KeyCode, KeyEvent};

use super::ModalOutcome;

#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub prompt: String,
}

impl ConfirmState {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
        }
    }

    pub const fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<bool> {
        match key.code {
            KeyCode::Char('y' | 'Y') => ModalOutcome::Commit(true),
            KeyCode::Char('n' | 'N') => ModalOutcome::Commit(false),
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn y_commits_true() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('y'))),
            ModalOutcome::Commit(true)
        ));
    }

    #[test]
    fn uppercase_y_commits_true() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('Y'))),
            ModalOutcome::Commit(true)
        ));
    }

    #[test]
    fn n_commits_false() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('n'))),
            ModalOutcome::Commit(false)
        ));
    }

    #[test]
    fn esc_cancels() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn arrow_is_noop() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Down)),
            ModalOutcome::Continue
        ));
    }
}

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const WHITE: Color = Color::Rgb(255, 255, 255);
const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);

pub fn render(frame: &mut Frame, area: Rect, state: &ConfirmState) {
    // Outer block
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_GREEN))
        .title(Span::styled(
            " Confirm ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    // Vertical layout inside the inner rect:
    //   prompt (1)
    //   spacer (1)
    //   button row (1)
    //   flex (remainder)
    //   footer hint (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // prompt
            Constraint::Length(1), // spacer
            Constraint::Length(1), // button row
            Constraint::Min(0),    // flex
            Constraint::Length(1), // footer
        ])
        .split(inner);

    // Prompt
    let prompt = Paragraph::new(Span::styled(
        state.prompt.clone(),
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(prompt, chunks[0]);

    // Button row — Yes (phosphor-on-black) and No default (white-on-black).
    let yes_btn = Span::styled(
        "  Yes  ",
        Style::default()
            .bg(PHOSPHOR_GREEN)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
    );
    let no_btn = Span::styled(
        "  No (default)  ",
        Style::default()
            .bg(WHITE)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
    );
    let button_line = Line::from(vec![yes_btn, Span::raw("    "), no_btn]);
    frame.render_widget(
        Paragraph::new(button_line).alignment(Alignment::Center),
        chunks[2],
    );

    // Footer hint — dim italic keyboard legend.
    let hint = Paragraph::new(Span::styled(
        "Y yes · N no · Esc cancel",
        Style::default()
            .fg(PHOSPHOR_DIM)
            .add_modifier(Modifier::ITALIC),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(hint, chunks[4]);
}
