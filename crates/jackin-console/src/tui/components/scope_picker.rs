// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace-vs-specific-role choice for the Secrets-tab Add flow.

use crossterm::event::{KeyCode, KeyEvent};

use jackin_tui::ModalOutcome;

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

use jackin_tui::components::render_dialog_shell;

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &ScopePickerState) {
    let inner = render_dialog_shell(frame, area, Some(state.title));

    // inner area is 3 rows (5 outer − 2 border): blank, button, blank.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top blank
            Constraint::Length(1), // button
            Constraint::Length(1), // bottom blank
        ])
        .split(inner);
    let items = [
        jackin_tui::components::ButtonStripItem::new("All roles"),
        jackin_tui::components::ButtonStripItem::new("Specific role"),
    ];
    let focused = match state.focused {
        ScopeChoice::AllAgents => 0,
        ScopeChoice::SpecificAgent => 1,
    };
    jackin_tui::components::ButtonStrip::new(&items)
        .focused(focused)
        .render(frame, chunks[1]);
}

#[cfg(test)]
mod tests;
