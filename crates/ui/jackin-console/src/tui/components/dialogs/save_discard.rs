// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `SaveDiscardState` and rendering.

use jackin_oppicker::ModalOutcome;
use ratatui::{Frame, layout::Rect, style::Style, text::Text};
use termrock::{
    input::{KeyCode, KeyEvent},
    widgets::{Action, ChoiceDialog, ChoiceDialogState, Dialog, PanelChrome},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDiscardChoice {
    Save,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveDiscardFocus {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct SaveDiscardState {
    pub prompt: String,
    focus: SaveDiscardFocus,
}

impl SaveDiscardState {
    #[must_use]
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            focus: SaveDiscardFocus::Cancel,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<SaveDiscardChoice> {
        match key.code {
            KeyCode::Char('s' | 'S') => ModalOutcome::Commit(SaveDiscardChoice::Save),
            KeyCode::Char('d' | 'D') => ModalOutcome::Commit(SaveDiscardChoice::Discard),
            KeyCode::Esc | KeyCode::Char('c' | 'C') => ModalOutcome::Cancel,
            KeyCode::Left | KeyCode::BackTab => {
                self.focus = match self.focus {
                    SaveDiscardFocus::Save => SaveDiscardFocus::Cancel,
                    SaveDiscardFocus::Discard => SaveDiscardFocus::Save,
                    SaveDiscardFocus::Cancel => SaveDiscardFocus::Discard,
                };
                ModalOutcome::Continue
            }
            KeyCode::Right | KeyCode::Tab => {
                self.focus = match self.focus {
                    SaveDiscardFocus::Save => SaveDiscardFocus::Discard,
                    SaveDiscardFocus::Discard => SaveDiscardFocus::Cancel,
                    SaveDiscardFocus::Cancel => SaveDiscardFocus::Save,
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
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Decision {
        Save,
        Discard,
        Cancel,
    }
    let actions = [
        Action {
            id: Decision::Save,
            label: "Save",
            enabled: true,
            style: None,
        },
        Action {
            id: Decision::Discard,
            label: "Discard",
            enabled: true,
            style: None,
        },
        Action {
            id: Decision::Cancel,
            label: "Cancel",
            enabled: true,
            style: None,
        },
    ];
    let focused = match state.focus {
        SaveDiscardFocus::Save => Decision::Save,
        SaveDiscardFocus::Discard => Decision::Discard,
        SaveDiscardFocus::Cancel => Decision::Cancel,
    };
    let theme = termrock::style::DesignSystem::default();
    let dialog = Dialog::new("Unsaved changes", Text::from(state.prompt.clone()), &theme)
        .style(Style::default())
        .emphasis(PanelChrome::Focused);
    frame.render_stateful_widget(
        &ChoiceDialog::new(dialog, &actions).gap(" "),
        area,
        &mut ChoiceDialogState::new(Some(focused)),
    );
}
