// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders},
};

use jackin_oppicker::ModalOutcome;
use termrock::layout::render_dialog_shell;
use termrock::scroll::ScrollAxes;
use termrock::scroll::{
    apply_scroll_delta, clamp_scroll_offset, is_scrollable, render_lines_with_offset_in_area,
};
use termrock::widgets::PanelChrome;
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
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        ConfirmSaveAction::Activate,
        Some("select"),
        Visibility::Shown,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        ConfirmSaveAction::Save,
        Some("save"),
        Visibility::Shown,
        Some("S"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('c')),
            KeyChord::plain(KeyCode::Char('C')),
        ],
        ConfirmSaveAction::Cancel,
        Some("cancel"),
        Visibility::Shown,
        Some("C/Esc"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Esc)],
        ConfirmSaveAction::Cancel,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Tab),
            KeyChord::plain(KeyCode::Right),
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('L')),
        ],
        ConfirmSaveAction::FocusNext,
        Some("move"),
        Visibility::Shown,
        Some("⇥/→"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::BackTab),
            KeyChord::plain(KeyCode::Left),
            KeyChord::plain(KeyCode::Char('h')),
            KeyChord::plain(KeyCode::Char('H')),
        ],
        ConfirmSaveAction::FocusPrev,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Up),
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        ConfirmSaveAction::ScrollUp,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Down),
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        ConfirmSaveAction::ScrollDown,
        None,
        Visibility::HiddenAlias,
        None,
    ),
];

pub static CONFIRM_SAVE_KEYMAP: Keymap<ConfirmSaveAction> =
    Keymap::from_static(CONFIRM_SAVE_BINDINGS);

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

#[derive(Debug, Clone)]
pub struct ConfirmSaveState<M: Clone = ()> {
    pub lines: Vec<Line<'static>>,
    pub focus: ConfirmSaveFocus,

    pub scroll_offset: u16,
    preview_rows: u16,

    pub effective_removals: Vec<String>,

    pub final_mounts: Option<Vec<M>>,

    pub has_collapses: bool,
}

impl<M: Clone> ConfirmSaveState<M> {
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
        match crate::tui::keymap::bridged_keymap_action(
            &CONFIRM_SAVE_KEYMAP,
            termrock::input::KeyEvent::from(key),
        ) {
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

#[must_use]
pub fn required_height<M: Clone>(state: &ConfirmSaveState<M>) -> u16 {
    let lines = u16::try_from(state.lines.len()).unwrap_or(u16::MAX);
    lines.saturating_add(6)
}

pub fn prepare_for_render<M: Clone>(area: Rect, state: &mut ConfirmSaveState<M>) {
    let inner = Block::default().borders(Borders::ALL).inner(area);

    let content_rows = inner.height.saturating_sub(4);
    state.preview_rows = content_rows;
    clamp_scroll_offset(
        state.lines.len(),
        usize::from(state.preview_rows),
        &mut state.scroll_offset,
    );
}

pub fn render<M: Clone>(frame: &mut Frame<'_>, area: Rect, state: &ConfirmSaveState<M>) {
    let inner = render_dialog_shell(
        frame,
        area,
        Some("Confirm changes"),
        PanelChrome::Focused,
        &termrock::style::DesignSystem::default(),
    );

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

    let (content, action_row) = crate::tui::dialog_layout::dialog_content_and_actions(inner);
    let theme = termrock::style::DesignSystem::default();

    render_lines_with_offset_in_area(frame, content, &indented, state.scroll_offset, &theme);

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
    frame.render_stateful_widget(
        &ActionBar::new(&actions, &theme).gap(" "),
        action_row,
        &mut ActionBarState {
            cursor: Some(state.focus),
            regions: Vec::new(),
        },
    );
}

#[cfg(test)]
mod tests;
