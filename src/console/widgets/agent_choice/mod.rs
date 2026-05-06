//! Two-row picker: Claude or Codex, used in the Auth-tab "+ Add" flow.
//!
//! Arrow keys move focus, Enter commits, Esc cancels.

use crate::agent::Agent;
use crate::console::widgets::ModalOutcome;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

#[derive(Debug, Clone)]
pub struct AgentChoiceState {
    pub focused: Agent,
}

impl AgentChoiceState {
    pub const fn new() -> Self {
        Self {
            focused: Agent::Claude,
        }
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<Agent> {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                if matches!(self.focused, Agent::Claude) {
                    self.focused = Agent::Codex;
                }
                ModalOutcome::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if matches!(self.focused, Agent::Codex) {
                    self.focused = Agent::Claude;
                }
                ModalOutcome::Continue
            }
            KeyCode::Enter => ModalOutcome::Commit(self.focused),
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }
}

impl Default for AgentChoiceState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &AgentChoiceState) {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let phosphor = Style::default().fg(ratatui::style::Color::Rgb(0, 255, 65));
    let make_row = |agent: Agent, label: &str| {
        let prefix = if state.focused == agent { "▸ " } else { "  " };
        Line::from(vec![
            Span::styled(prefix, phosphor),
            Span::styled(label.to_string(), bold),
        ])
    };
    let lines = vec![
        make_row(Agent::Claude, "Claude"),
        make_row(Agent::Codex, "Codex"),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Pick agent ");
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn down_moves_to_codex_then_clamps() {
        let mut s = AgentChoiceState::new();
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, Agent::Codex);
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, Agent::Codex);
    }

    #[test]
    fn up_moves_to_claude_then_clamps() {
        let mut s = AgentChoiceState::new();
        s.focused = Agent::Codex;
        let _ = s.handle_key(key(KeyCode::Up));
        assert_eq!(s.focused, Agent::Claude);
        let _ = s.handle_key(key(KeyCode::Up));
        assert_eq!(s.focused, Agent::Claude);
    }

    #[test]
    fn enter_commits_focused_agent() {
        let mut s = AgentChoiceState::new();
        s.focused = Agent::Codex;
        match s.handle_key(key(KeyCode::Enter)) {
            ModalOutcome::Commit(a) => assert_eq!(a, Agent::Codex),
            other => panic!("expected commit, got {other:?}"),
        }
    }

    #[test]
    fn esc_cancels() {
        let mut s = AgentChoiceState::new();
        assert!(matches!(s.handle_key(key(KeyCode::Esc)), ModalOutcome::Cancel));
    }
}
