// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Agent picker used in the Auth-tab "+ Add" flow.
//!
//! Arrow keys move focus, Enter commits, Esc cancels.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use termrock::{ModalOutcome, components::render_picker_lines};

use termrock::components::{DialogBorder, render_dialog_shell};

pub trait AgentChoice: Copy + Eq + 'static {
    const ALL: &'static [Self];

    fn label(self) -> &'static str;
}

impl AgentChoice for jackin_core::Agent {
    const ALL: &'static [Self] = jackin_core::Agent::ALL;

    fn label(self) -> &'static str {
        self.label()
    }
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
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                let idx = focus_index_in(&self.choices, self.focused);
                if idx + 1 < self.choices.len() {
                    self.focused = self.choices[idx + 1];
                }
                ModalOutcome::Continue
            }
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
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

pub fn render<A: AgentChoice>(frame: &mut Frame<'_>, area: Rect, state: &AgentChoiceState<A>) {
    let bold = Style::default().add_modifier(Modifier::BOLD);

    let inner = render_dialog_shell(frame, area, Some("Pick Agent"), DialogBorder::Default);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // agent list
            Constraint::Length(1), // spacer
        ])
        .split(inner);

    let lines: Vec<Line<'_>> = state
        .choices
        .iter()
        .map(|a| Line::from(Span::styled(agent_picker_label(*a).to_owned(), bold)))
        .collect();
    render_picker_lines(
        rows[0],
        frame.buffer_mut(),
        lines,
        Some(focus_index_in(&state.choices, state.focused)),
    );
}

#[cfg(test)]
mod tests;
