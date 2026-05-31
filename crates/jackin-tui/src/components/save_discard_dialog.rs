//! Three-way dirty-exit confirmation dialog.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::Paragraph,
};

use crate::{ModalOutcome, theme::WHITE};

use super::button_strip::{ButtonStrip, ButtonStripItem};
use super::panel::modal_block;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDiscardChoice {
    Save,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDiscardFocus {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct SaveDiscardState {
    pub prompt: String,
    pub focus: SaveDiscardFocus,
}

impl SaveDiscardState {
    /// Default focus = Cancel so accidental Enter does not discard work.
    #[must_use]
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            focus: SaveDiscardFocus::Cancel,
        }
    }

    pub const fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<SaveDiscardChoice> {
        match key.code {
            KeyCode::Char('s' | 'S') => ModalOutcome::Commit(SaveDiscardChoice::Save),
            KeyCode::Char('d' | 'D') => ModalOutcome::Commit(SaveDiscardChoice::Discard),
            KeyCode::Char('c' | 'C') | KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l' | 'L') => {
                self.focus = match self.focus {
                    SaveDiscardFocus::Save => SaveDiscardFocus::Discard,
                    SaveDiscardFocus::Discard => SaveDiscardFocus::Cancel,
                    SaveDiscardFocus::Cancel => SaveDiscardFocus::Save,
                };
                ModalOutcome::Continue
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h' | 'H') => {
                self.focus = match self.focus {
                    SaveDiscardFocus::Save => SaveDiscardFocus::Cancel,
                    SaveDiscardFocus::Discard => SaveDiscardFocus::Save,
                    SaveDiscardFocus::Cancel => SaveDiscardFocus::Discard,
                };
                ModalOutcome::Continue
            }
            KeyCode::Enter => match self.focus {
                SaveDiscardFocus::Save => ModalOutcome::Commit(SaveDiscardChoice::Save),
                SaveDiscardFocus::Discard => ModalOutcome::Commit(SaveDiscardChoice::Discard),
                SaveDiscardFocus::Cancel => ModalOutcome::Cancel,
            },
            _ => ModalOutcome::Continue,
        }
    }
}

pub fn render_save_discard_dialog(frame: &mut Frame<'_>, area: Rect, state: &SaveDiscardState) {
    let block = modal_block().title(Span::styled(
        " Unsaved changes ",
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Span::styled(
            state.prompt.clone(),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        chunks[0],
    );

    let items = [
        ButtonStripItem::new("Save"),
        ButtonStripItem::new("Discard"),
        ButtonStripItem::new("Cancel"),
    ];
    let focused = match state.focus {
        SaveDiscardFocus::Save => 0,
        SaveDiscardFocus::Discard => 1,
        SaveDiscardFocus::Cancel => 2,
    };
    ButtonStrip::new(&items)
        .focused(focused)
        .render(frame, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn default_focus_is_cancel() {
        let s = SaveDiscardState::new("?");
        assert_eq!(s.focus, SaveDiscardFocus::Cancel);
    }

    #[test]
    fn shortcuts_commit_or_cancel() {
        let mut s = SaveDiscardState::new("?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('s'))),
            ModalOutcome::Commit(SaveDiscardChoice::Save)
        ));
        let mut s = SaveDiscardState::new("?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('d'))),
            ModalOutcome::Commit(SaveDiscardChoice::Discard)
        ));
        let mut s = SaveDiscardState::new("?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn enter_commits_focused_button() {
        let mut s = SaveDiscardState::new("?");
        let _ = s.handle_key(key(KeyCode::Tab));
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Commit(SaveDiscardChoice::Save)
        ));

        let mut s = SaveDiscardState::new("?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Cancel
        ));
    }
}
