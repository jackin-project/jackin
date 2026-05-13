//! Agent picker used in the Auth-tab "+ Add" flow.
//!
//! Arrow keys move focus, Enter commits, Esc cancels.

use crate::agent::Agent;
use crate::console::widgets::ModalOutcome;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::{PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE};

#[derive(Debug, Clone)]
pub struct AgentChoiceState {
    pub choices: Vec<Agent>,
    pub focused: Agent,
}

impl AgentChoiceState {
    pub fn new() -> Self {
        Self::with_choices(Agent::ALL.to_vec())
    }

    pub fn with_choices(choices: Vec<Agent>) -> Self {
        let choices = if choices.is_empty() {
            Agent::ALL.to_vec()
        } else {
            choices
        };
        let focused = choices[0];
        Self { choices, focused }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<Agent> {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                let idx = focus_index_in(&self.choices, self.focused);
                if idx + 1 < self.choices.len() {
                    self.focused = self.choices[idx + 1];
                }
                ModalOutcome::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let idx = focus_index_in(&self.choices, self.focused);
                if idx > 0 {
                    self.focused = self.choices[idx - 1];
                }
                ModalOutcome::Continue
            }
            KeyCode::Enter => ModalOutcome::Commit(self.focused),
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }
}

fn focus_index_in(choices: &[Agent], agent: Agent) -> usize {
    choices.iter().position(|a| *a == agent).unwrap_or(0)
}

pub const fn agent_picker_label(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "Claude",
        Agent::Codex => "Codex",
        Agent::Amp => "Amp",
        Agent::Opencode => "OpenCode",
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

    let lines: Vec<Line> = state
        .choices
        .iter()
        .map(|a| make_row(*a, agent_picker_label(*a)))
        .collect();
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
    fn down_moves_through_agents_then_clamps() {
        let mut s = AgentChoiceState::new();
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, Agent::Codex);
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, Agent::Amp);
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, Agent::Opencode);
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, Agent::Opencode);
    }

    #[test]
    fn up_moves_through_agents_then_clamps() {
        let mut s = AgentChoiceState::new();
        s.focused = Agent::Opencode;
        let _ = s.handle_key(key(KeyCode::Up));
        assert_eq!(s.focused, Agent::Amp);
        let _ = s.handle_key(key(KeyCode::Up));
        assert_eq!(s.focused, Agent::Codex);
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
    fn with_choices_limits_navigation_and_default_focus() {
        let mut s = AgentChoiceState::with_choices(vec![Agent::Codex, Agent::Amp]);
        assert_eq!(s.focused, Agent::Codex);
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, Agent::Amp);
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, Agent::Amp);
        let _ = s.handle_key(key(KeyCode::Up));
        assert_eq!(s.focused, Agent::Codex);
    }

    #[test]
    fn empty_choices_falls_back_to_agent_all() {
        let s = AgentChoiceState::with_choices(Vec::new());
        assert_eq!(s.choices, Agent::ALL.to_vec());
        assert_eq!(s.focused, Agent::ALL[0]);
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
