// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Three-way dirty-exit confirmation dialog.

use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::Span,
    widgets::Paragraph,
};

use crate::keymap::{KeyBinding, KeyChord, Keymap, LogicalKey, Visibility};
use crate::{HintSpan, ModalOutcome, components::ButtonFocus};

use super::button_strip::{ButtonStrip, ButtonStripItem};
use super::dialog_layout::{DialogBorder, dialog_inner_chunks, render_dialog_shell};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDiscardChoice {
    Save,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDiscardAction {
    Save,
    Discard,
    Cancel,
    FocusNext,
    FocusPrev,
    CommitFocused,
}

const SAVE_DISCARD_BINDINGS: &[KeyBinding<SaveDiscardAction>] = &[
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Enter)],
        action: SaveDiscardAction::CommitFocused,
        hint: Some("select"),
        visibility: Visibility::Shown,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('s')),
            KeyChord::plain(LogicalKey::Char('S')),
        ],
        action: SaveDiscardAction::Save,
        hint: Some("save"),
        visibility: Visibility::Shown,
        glyph: Some("S"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('d')),
            KeyChord::plain(LogicalKey::Char('D')),
        ],
        action: SaveDiscardAction::Discard,
        hint: Some("discard"),
        visibility: Visibility::Shown,
        glyph: Some("D"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('c')),
            KeyChord::plain(LogicalKey::Char('C')),
        ],
        action: SaveDiscardAction::Cancel,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Esc)],
        action: SaveDiscardAction::Cancel,
        hint: Some("cancel"),
        visibility: Visibility::Shown,
        glyph: Some("Esc"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Tab),
            KeyChord::plain(LogicalKey::Right),
            KeyChord::plain(LogicalKey::Char('l')),
            KeyChord::plain(LogicalKey::Char('L')),
        ],
        action: SaveDiscardAction::FocusNext,
        hint: Some("move"),
        visibility: Visibility::Shown,
        glyph: Some("⇥/→"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::BackTab),
            KeyChord::plain(LogicalKey::Left),
            KeyChord::plain(LogicalKey::Char('h')),
            KeyChord::plain(LogicalKey::Char('H')),
        ],
        action: SaveDiscardAction::FocusPrev,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
];

pub static SAVE_DISCARD_KEYMAP: Keymap<SaveDiscardAction> = Keymap::new(SAVE_DISCARD_BINDINGS);

#[must_use]
pub fn save_discard_hint_spans() -> Vec<HintSpan<'static>> {
    SAVE_DISCARD_KEYMAP.hint_spans()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDiscardFocus {
    Save,
    Discard,
    Cancel,
}

impl ButtonFocus for SaveDiscardFocus {
    const RING: &'static [Self] = &[Self::Save, Self::Discard, Self::Cancel];
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

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<SaveDiscardChoice> {
        match SAVE_DISCARD_KEYMAP.dispatch(KeyChord::from(key)) {
            Some(SaveDiscardAction::Save) => ModalOutcome::Commit(SaveDiscardChoice::Save),
            Some(SaveDiscardAction::Discard) => ModalOutcome::Commit(SaveDiscardChoice::Discard),
            Some(SaveDiscardAction::Cancel) => ModalOutcome::Cancel,
            Some(SaveDiscardAction::FocusNext) => {
                self.focus = self.focus.next();
                ModalOutcome::Continue
            }
            Some(SaveDiscardAction::FocusPrev) => {
                self.focus = self.focus.prev();
                ModalOutcome::Continue
            }
            Some(SaveDiscardAction::CommitFocused) => match self.focus {
                SaveDiscardFocus::Save => ModalOutcome::Commit(SaveDiscardChoice::Save),
                SaveDiscardFocus::Discard => ModalOutcome::Commit(SaveDiscardChoice::Discard),
                SaveDiscardFocus::Cancel => ModalOutcome::Cancel,
            },
            None => ModalOutcome::Continue,
        }
    }
}

pub fn render_save_discard_dialog(frame: &mut Frame<'_>, area: Rect, state: &SaveDiscardState) {
    let inner = render_dialog_shell(frame, area, Some("Unsaved changes"), DialogBorder::Default);
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
    frame.render_widget(
        ButtonStrip::new(&items).focused(state.focus.index()),
        chunks[3],
    );
}

#[cfg(test)]
mod tests;
