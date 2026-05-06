//! Two-row picker: Claude or Codex, used in the Auth-tab "+ Add" flow.
//!
//! Arrow keys move focus, Enter commits, Esc cancels.

use crate::agent::Agent;
use crate::console::widgets::ModalOutcome;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const WHITE: Color = Color::Rgb(255, 255, 255);

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

    pub const fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<Agent> {
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
    let phosphor = Style::default().fg(PHOSPHOR_GREEN);
    let make_row = |agent: Agent, label: &str| {
        let prefix = if state.focused == agent { "▸ " } else { "  " };
        Line::from(vec![
            Span::styled(prefix, phosphor),
            Span::styled(label.to_string(), bold),
        ])
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " Pick Agent ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // agent list
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hint footer
        ])
        .split(inner);

    let lines = vec![
        make_row(Agent::Claude, "Claude"),
        make_row(Agent::Codex, "Codex"),
    ];
    frame.render_widget(Paragraph::new(lines), rows[0]);

    let key_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(PHOSPHOR_GREEN);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("Enter", key_style),
        Span::styled(" commit", text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("Esc", key_style),
        Span::styled(" cancel", text_style),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(hint, rows[2]);
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
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }
}
