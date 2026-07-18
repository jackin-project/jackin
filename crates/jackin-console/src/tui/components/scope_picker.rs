// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace-vs-specific-role choice for the Secrets-tab Add flow.

use crossterm::event::{KeyCode, KeyEvent};

use jackin_tui::ModalOutcome;
use termrock::widgets::{Action, ActionBar, ActionBarState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeChoice {
    AllAgents,
    SpecificAgent,
}

#[derive(Debug, Clone)]
pub struct ScopePickerState {
    pub focused: ScopeChoice,
    pub title: &'static str,
}

impl Default for ScopePickerState {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopePickerState {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            focused: ScopeChoice::AllAgents,
            title: " New environment variable ",
        }
    }

    #[must_use]
    pub const fn with_title(title: &'static str) -> Self {
        Self {
            focused: ScopeChoice::AllAgents,
            title,
        }
    }

    pub const fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<ScopeChoice> {
        match key.code {
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Right
            | KeyCode::Left
            | KeyCode::Char('l' | 'L' | 'h' | 'H') => {
                self.cycle();
                ModalOutcome::Continue
            }
            KeyCode::Enter => ModalOutcome::Commit(self.focused),
            _ => ModalOutcome::Continue,
        }
    }

    const fn cycle(&mut self) {
        self.focused = match self.focused {
            ScopeChoice::AllAgents => ScopeChoice::SpecificAgent,
            ScopeChoice::SpecificAgent => ScopeChoice::AllAgents,
        };
    }
}

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use termrock::layout::render_dialog_shell;
use termrock::widgets::PanelEmphasis;

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &ScopePickerState) {
    let inner = render_dialog_shell(
        frame,
        area,
        Some(state.title),
        PanelEmphasis::Focused,
        &termrock::Theme::default(),
    );

    // inner area is 3 rows (5 outer − 2 border): blank, button, blank.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top blank
            Constraint::Length(1), // button
            Constraint::Length(1), // bottom blank
        ])
        .split(inner);
    let actions = [
        Action {
            id: ScopeChoice::AllAgents,
            label: "All roles",
            enabled: true,
            style: None,
        },
        Action {
            id: ScopeChoice::SpecificAgent,
            label: "Specific role",
            enabled: true,
            style: None,
        },
    ];
    let theme = termrock::Theme::default();
    frame.render_stateful_widget(
        &ActionBar::new(&actions, &theme).gap(" "),
        chunks[1],
        &mut ActionBarState {
            focused: Some(state.focused),
            regions: Vec::new(),
        },
    );
}

#[cfg(test)]
mod tests;
