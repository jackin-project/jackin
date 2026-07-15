// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Plain-or-1Password choice between `EnvKey` input and value entry.

use crossterm::event::{KeyCode, KeyEvent};

use jackin_tui::ModalOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceChoice {
    Plain,
    Op,
}

#[derive(Debug, Clone)]
pub struct SourcePickerState {
    pub key: String,
    /// Captured from `ConsoleState::op_available` (probed once at
    /// startup); operator must restart to pick up a mid-session
    /// install. When `false`, the Op button renders dim and `←`/`→`
    /// skip it.
    pub op_available: bool,
    pub focused: SourceChoice,
}

impl SourcePickerState {
    #[must_use]
    pub const fn new(key: String, op_available: bool) -> Self {
        Self {
            key,
            op_available,
            focused: SourceChoice::Plain,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<SourceChoice> {
        match key.code {
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Char('p' | 'P') => ModalOutcome::Commit(SourceChoice::Plain),
            KeyCode::Char('o' | 'O') if self.op_available => ModalOutcome::Commit(SourceChoice::Op),
            KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Right
            | KeyCode::Left
            | KeyCode::Char('l' | 'L' | 'h' | 'H') => {
                self.cycle();
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                // Defensive: `cycle` never parks focus on disabled Op,
                // but refuse to commit if a future code path does.
                if self.focused == SourceChoice::Op && !self.op_available {
                    return ModalOutcome::Continue;
                }
                ModalOutcome::Commit(self.focused)
            }
            _ => ModalOutcome::Continue,
        }
    }

    const fn cycle(&mut self) {
        if !self.op_available {
            return;
        }
        self.focused = match self.focused {
            SourceChoice::Plain => SourceChoice::Op,
            SourceChoice::Op => SourceChoice::Plain,
        };
    }
}

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::Paragraph,
};

use jackin_tui::components::{DialogBorder, render_dialog_shell};
use termrock::style::PHOSPHOR_DARK;

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &SourcePickerState) {
    let title = format!("Source for {}", state.key);
    let inner = render_dialog_shell(frame, area, Some(&title), DialogBorder::Default);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let items = [
        jackin_tui::components::ButtonStripItem::new("Plain text"),
        if state.op_available {
            jackin_tui::components::ButtonStripItem::new("1Password")
        } else {
            jackin_tui::components::ButtonStripItem::disabled("1Password")
        },
    ];
    let focused = match state.focused {
        SourceChoice::Plain => 0,
        SourceChoice::Op => 1,
    };
    frame.render_widget(
        jackin_tui::components::ButtonStrip::new(&items).focused(focused),
        chunks[1],
    );

    if !state.op_available {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "(install op CLI to enable)",
                Style::default()
                    .fg(PHOSPHOR_DARK)
                    .add_modifier(Modifier::DIM),
            ))
            .alignment(Alignment::Center),
            chunks[2],
        );
    }
}

#[cfg(test)]
mod tests;
