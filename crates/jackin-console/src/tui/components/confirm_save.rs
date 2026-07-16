// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Two-button confirmation modal shown before committing an editor save.
//!
//! Replaces the old inline "bare S → save immediately" flow with a
//! preview dialog: operator reviews a list of changes (pre-built by the
//! caller in [`ConfirmSaveState`]), then picks `Save` or `Cancel`.
//! Mount-collapse warnings fold into the same dialog as an extra
//! section so the operator sees ONE confirm for the full plan.

use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders},
};

use jackin_core::ModalOutcome;
use termrock::layout::{DialogBorder, ScrollAxes, dialog_inner_chunks, render_dialog_shell};
use termrock::scroll::{
    apply_scroll_delta, clamp_scroll_offset, is_scrollable, render_lines_with_offset_in_area,
};
use termrock::{
    input::KeyCode,
    keymap::{KeyBinding, KeyChord, Keymap, SCROLL_HINT_KEYMAP, Visibility},
    widgets::{Action, ActionBar, ActionBarState, HintSpan},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveChoice {
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmSaveAction {
    Save,
    Cancel,
    Activate,
    FocusNext,
    FocusPrev,
    ScrollUp,
    ScrollDown,
}

const CONFIRM_SAVE_BINDINGS: &[KeyBinding<ConfirmSaveAction>] = &[
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Enter)],
        action: ConfirmSaveAction::Activate,
        hint: Some("select"),
        visibility: Visibility::Shown,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        action: ConfirmSaveAction::Save,
        hint: Some("save"),
        visibility: Visibility::Shown,
        glyph: Some("S"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('c')),
            KeyChord::plain(KeyCode::Char('C')),
        ],
        action: ConfirmSaveAction::Cancel,
        hint: Some("cancel"),
        visibility: Visibility::Shown,
        glyph: Some("C/Esc"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Esc)],
        action: ConfirmSaveAction::Cancel,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Tab),
            KeyChord::plain(KeyCode::Right),
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('L')),
        ],
        action: ConfirmSaveAction::FocusNext,
        hint: Some("move"),
        visibility: Visibility::Shown,
        glyph: Some("⇥/→"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::BackTab),
            KeyChord::plain(KeyCode::Left),
            KeyChord::plain(KeyCode::Char('h')),
            KeyChord::plain(KeyCode::Char('H')),
        ],
        action: ConfirmSaveAction::FocusPrev,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Up),
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        action: ConfirmSaveAction::ScrollUp,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Down),
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        action: ConfirmSaveAction::ScrollDown,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
];

pub static CONFIRM_SAVE_KEYMAP: Keymap<ConfirmSaveAction> = Keymap::new(CONFIRM_SAVE_BINDINGS);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmSaveFocus {
    Save,
    Cancel,
}

impl ConfirmSaveFocus {
    const fn next(self) -> Self {
        match self {
            Self::Save => Self::Cancel,
            Self::Cancel => Self::Save,
        }
    }

    const fn prev(self) -> Self {
        self.next()
    }
}

/// State for the `ConfirmSave` modal. The caller pre-builds the content
/// `lines` from the editor state so the widget itself stays dumb.
///
/// `effective_removals` and `final_mounts` carry the planner's output
/// through the confirm step — `input.rs::save_editor` consumes them when
/// the operator commits, and no second `plan_edit`/`plan_create` call is
/// needed after confirmation.
#[derive(Debug, Clone)]
pub struct ConfirmSaveState<M: Clone = ()> {
    pub lines: Vec<Line<'static>>,
    pub focus: ConfirmSaveFocus,
    /// Vertical scroll offset — how many lines are hidden above the visible window.
    pub scroll_offset: u16,
    preview_rows: u16,
    /// `plan_edit`'s `effective_removals`, forwarded into
    /// `edit_workspace`. Empty for Create flows.
    pub effective_removals: Vec<String>,
    /// `plan_create`'s collapsed mount set. Empty (meaning "no override
    /// needed") for Edit flows.
    pub final_mounts: Option<Vec<M>>,
    /// `true` when the plan carries mount-collapses.
    pub has_collapses: bool,
}

impl<M: Clone> ConfirmSaveState<M> {
    /// Build a new `ConfirmSave` modal. Default focus = Cancel so that Enter
    /// on a freshly-opened confirm never fires the destructive arm (RULE 7).
    #[must_use]
    pub const fn new(lines: Vec<Line<'static>>) -> Self {
        Self {
            lines,
            focus: ConfirmSaveFocus::Cancel,
            scroll_offset: 0,
            preview_rows: 0,
            effective_removals: Vec::new(),
            final_mounts: None,
            has_collapses: false,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<SaveChoice> {
        match CONFIRM_SAVE_KEYMAP.dispatch(KeyChord::from(termrock::input::KeyEvent::from(key))) {
            Some(ConfirmSaveAction::Save) => ModalOutcome::Commit(SaveChoice::Save),
            Some(ConfirmSaveAction::Cancel) => ModalOutcome::Cancel,
            Some(ConfirmSaveAction::ScrollUp) => {
                self.scroll_preview_by(-1);
                ModalOutcome::Continue
            }
            Some(ConfirmSaveAction::ScrollDown) => {
                self.scroll_preview_by(1);
                ModalOutcome::Continue
            }
            Some(ConfirmSaveAction::FocusNext) => {
                self.focus = self.focus.next();
                ModalOutcome::Continue
            }
            Some(ConfirmSaveAction::FocusPrev) => {
                self.focus = self.focus.prev();
                ModalOutcome::Continue
            }
            Some(ConfirmSaveAction::Activate) => match self.focus {
                ConfirmSaveFocus::Save => ModalOutcome::Commit(SaveChoice::Save),
                ConfirmSaveFocus::Cancel => ModalOutcome::Cancel,
            },
            None => ModalOutcome::Continue,
        }
    }

    fn scroll_preview_by(&mut self, delta: isize) {
        apply_scroll_delta(
            &mut self.scroll_offset,
            delta as i16,
            usize::from(self.preview_rows),
            self.lines.len(),
        );
    }

    #[must_use]
    pub fn scroll_axes(&self) -> ScrollAxes {
        ScrollAxes {
            vertical: is_scrollable(self.lines.len(), usize::from(self.preview_rows)),
            horizontal: false,
        }
    }
}

#[must_use]
pub fn confirm_save_hint_spans<M: Clone>(state: &ConfirmSaveState<M>) -> Vec<HintSpan<'static>> {
    confirm_save_hint_spans_for_axes(state.scroll_axes())
}

#[must_use]
pub fn confirm_save_hint_spans_for_axes(scroll_axes: ScrollAxes) -> Vec<HintSpan<'static>> {
    let mut items = CONFIRM_SAVE_KEYMAP.hint_spans();
    let scroll_items = SCROLL_HINT_KEYMAP.hint_spans_for_axes(scroll_axes);
    if !scroll_items.is_empty() {
        items.push(HintSpan::GroupSep);
        items.extend(scroll_items);
    }
    items
}

/// Total rows the `ConfirmSave` modal wants given its current line count.
/// Layout: top border + blank + N content lines + blank + buttons + bottom border = N + 5.
#[must_use]
pub fn required_height<M: Clone>(state: &ConfirmSaveState<M>) -> u16 {
    // 2 borders + 1 leading + content + 1 spacer + 1 buttons + 1 trailing = lines + 6
    let lines = u16::try_from(state.lines.len()).unwrap_or(u16::MAX);
    lines.saturating_add(6)
}

pub fn prepare_for_render<M: Clone>(area: Rect, state: &mut ConfirmSaveState<M>) {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    // Subtract 4 fixed rows (leading + spacer + buttons + trailing).
    let content_rows = inner.height.saturating_sub(4);
    state.preview_rows = content_rows;
    clamp_scroll_offset(
        state.lines.len(),
        usize::from(state.preview_rows),
        &mut state.scroll_offset,
    );
}

pub fn render<M: Clone>(frame: &mut Frame<'_>, area: Rect, state: &ConfirmSaveState<M>) {
    let inner = render_dialog_shell(frame, area, Some("Confirm changes"), DialogBorder::Default);

    // Content indented by SUBPANEL_CONTENT_INDENT (2). The caller is
    // responsible for any deeper indentation; we just add a uniform
    // left gutter so lines don't butt up against the border.
    let indented: Vec<Line<'_>> = state
        .lines
        .iter()
        .cloned()
        .map(|l| {
            let mut spans = vec![Span::raw("  ")];
            spans.extend(l.spans);
            Line::from(spans)
        })
        .collect();

    let chunks = dialog_inner_chunks(inner, None);

    render_lines_with_offset_in_area(frame, chunks[1], indented, state.scroll_offset);

    let actions = [
        Action {
            id: ConfirmSaveFocus::Save,
            label: "Save",
            enabled: true,
            style: None,
        },
        Action {
            id: ConfirmSaveFocus::Cancel,
            label: "Cancel",
            enabled: true,
            style: None,
        },
    ];
    let theme = termrock::Theme::default();
    frame.render_stateful_widget(
        &ActionBar::new(&actions, &theme).gap(" "),
        chunks[3],
        &mut ActionBarState {
            focused: Some(state.focus),
            regions: Vec::new(),
        },
    );
}

#[cfg(test)]
mod tests;
