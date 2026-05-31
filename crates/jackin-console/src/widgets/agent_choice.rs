//! Agent picker used in the Auth-tab "+ Add" flow.
//!
//! Arrow keys move focus, Enter commits, Esc cancels.

use crate::widgets::ModalOutcome;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::PHOSPHOR_GREEN;
use jackin_tui::components::{Panel, PanelFocus};

pub trait AgentChoice: Copy + Eq + 'static {
    const ALL: &'static [Self];

    fn label(self) -> &'static str;
}

#[derive(Debug, Clone)]
pub struct AgentChoiceState<A: AgentChoice> {
    pub choices: Vec<A>,
    pub focused: A,
}

impl<A: AgentChoice> AgentChoiceState<A> {
    pub fn new() -> Self {
        Self::with_choices(A::ALL.to_vec())
    }

    pub fn with_choices(choices: Vec<A>) -> Self {
        let choices = if choices.is_empty() {
            A::ALL.to_vec()
        } else {
            choices
        };
        let focused = choices[0];
        Self { choices, focused }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<A> {
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

fn focus_index_in<A: AgentChoice>(choices: &[A], agent: A) -> usize {
    choices.iter().position(|a| *a == agent).unwrap_or(0)
}

pub fn agent_picker_label<A: AgentChoice>(agent: A) -> &'static str {
    agent.label()
}

impl<A: AgentChoice> Default for AgentChoiceState<A> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render<A: AgentChoice>(frame: &mut Frame, area: Rect, state: &AgentChoiceState<A>) {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let phosphor = Style::default().fg(PHOSPHOR_GREEN);
    let make_row = |agent: A, label: &str| {
        let prefix = if state.focused == agent { "▸ " } else { "  " };
        Line::from(vec![
            Span::styled(prefix, phosphor),
            Span::styled(label.to_string(), bold),
        ])
    };

    let block = Panel::new()
        .title(" Pick Agent ")
        .focus(PanelFocus::Focused)
        .block();

    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // agent list
            Constraint::Length(1), // spacer
        ])
        .split(inner);

    let lines: Vec<Line> = state
        .choices
        .iter()
        .map(|a| make_row(*a, agent_picker_label(*a)))
        .collect();
    frame.render_widget(Paragraph::new(lines), rows[0]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestAgent {
        Claude,
        Codex,
        Amp,
        Kimi,
        Opencode,
    }

    impl AgentChoice for TestAgent {
        const ALL: &'static [Self] = &[
            Self::Claude,
            Self::Codex,
            Self::Amp,
            Self::Kimi,
            Self::Opencode,
        ];

        fn label(self) -> &'static str {
            match self {
                Self::Claude => "Claude",
                Self::Codex => "Codex",
                Self::Amp => "Amp",
                Self::Kimi => "Kimi",
                Self::Opencode => "OpenCode",
            }
        }
    }

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
        let mut s = AgentChoiceState::<TestAgent>::new();
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, TestAgent::Codex);
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, TestAgent::Amp);
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, TestAgent::Kimi);
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, TestAgent::Opencode);
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, TestAgent::Opencode);
    }

    #[test]
    fn up_moves_through_agents_then_clamps() {
        let mut s = AgentChoiceState::<TestAgent>::new();
        s.focused = TestAgent::Opencode;
        let _ = s.handle_key(key(KeyCode::Up));
        assert_eq!(s.focused, TestAgent::Kimi);
        let _ = s.handle_key(key(KeyCode::Up));
        assert_eq!(s.focused, TestAgent::Amp);
        let _ = s.handle_key(key(KeyCode::Up));
        assert_eq!(s.focused, TestAgent::Codex);
        let _ = s.handle_key(key(KeyCode::Up));
        assert_eq!(s.focused, TestAgent::Claude);
        let _ = s.handle_key(key(KeyCode::Up));
        assert_eq!(s.focused, TestAgent::Claude);
    }

    #[test]
    fn enter_commits_focused_agent() {
        let mut s = AgentChoiceState::<TestAgent>::new();
        s.focused = TestAgent::Codex;
        match s.handle_key(key(KeyCode::Enter)) {
            ModalOutcome::Commit(a) => assert_eq!(a, TestAgent::Codex),
            other => panic!("expected commit, got {other:?}"),
        }
    }

    #[test]
    fn with_choices_limits_navigation_and_default_focus() {
        let mut s = AgentChoiceState::with_choices(vec![TestAgent::Codex, TestAgent::Amp]);
        assert_eq!(s.focused, TestAgent::Codex);
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, TestAgent::Amp);
        let _ = s.handle_key(key(KeyCode::Down));
        assert_eq!(s.focused, TestAgent::Amp);
        let _ = s.handle_key(key(KeyCode::Up));
        assert_eq!(s.focused, TestAgent::Codex);
    }

    #[test]
    fn empty_choices_falls_back_to_agent_all() {
        let s = AgentChoiceState::<TestAgent>::with_choices(Vec::new());
        assert_eq!(s.choices, TestAgent::ALL.to_vec());
        assert_eq!(s.focused, TestAgent::ALL[0]);
    }

    #[test]
    fn esc_cancels() {
        let mut s = AgentChoiceState::<TestAgent>::new();
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }
}
