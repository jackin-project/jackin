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
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);

pub fn render(frame: &mut Frame, area: Rect, state: &ConfirmState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_GREEN))
        .title("Confirm");

    let body = format!("{}\n\n[Y]es · [N]o (default) · Esc cancel", state.prompt);

    let paragraph = Paragraph::new(body)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));

    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(paragraph, area);
}
