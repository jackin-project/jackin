//! Three-way dirty-exit confirmation dialog.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::Span,
    widgets::Paragraph,
};

use crate::ModalOutcome;

use super::button_strip::{ButtonStrip, ButtonStripItem};
use super::dialog_layout::{dialog_inner_chunks, render_dialog_shell};

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
    let inner = render_dialog_shell(frame, area, Some("Unsaved changes"));
    let chunks = dialog_inner_chunks(inner, Some(1));

    frame.render_widget(
        Paragraph::new(Span::styled(state.prompt.clone(), crate::theme::BOLD_WHITE))
            .alignment(Alignment::Center),
        chunks[1],
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
        .render(frame, chunks[3]);
}

#[cfg(test)]
mod tests;
