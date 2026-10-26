// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use termrock::input::{KeyBinding, KeyChord, KeyCode, Keymap, Visibility};
use termrock::keymap::glyph;

pub(crate) fn raw_bytes_to_chord(bytes: &[u8]) -> Option<KeyChord> {
    termrock::keymap::raw_bytes_to_chord(bytes)
}

use crate::tui::input::InputEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalCapsuleAction {
    RequestExit,
}

impl GlobalCapsuleAction {
    pub(crate) fn to_input_event(self) -> InputEvent {
        match self {
            GlobalCapsuleAction::RequestExit => InputEvent::RequestExit,
        }
    }
}

pub(crate) static CAPSULE_GLOBAL_KEYMAP_BINDINGS: &[KeyBinding<GlobalCapsuleAction>] =
    &[KeyBinding::borrowed(
        &[KeyChord::ctrl(KeyCode::Char('q'))],
        GlobalCapsuleAction::RequestExit,
        Some("quit"),
        Visibility::Shown,
        None,
    )];
pub(crate) static CAPSULE_GLOBAL_KEYMAP: Keymap<GlobalCapsuleAction> =
    Keymap::from_static(CAPSULE_GLOBAL_KEYMAP_BINDINGS);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResizePaneAction {
    Up,
    Down,
    Left,
    Right,
}

impl ResizePaneAction {
    pub(crate) fn to_input_event(self) -> InputEvent {
        use crate::tui::input::ArrowDir;
        match self {
            Self::Up => InputEvent::ResizePane(ArrowDir::Up),
            Self::Down => InputEvent::ResizePane(ArrowDir::Down),
            Self::Left => InputEvent::ResizePane(ArrowDir::Left),
            Self::Right => InputEvent::ResizePane(ArrowDir::Right),
        }
    }
}

pub(crate) static RESIZE_PANE_KEYMAP_BINDINGS: &[KeyBinding<ResizePaneAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::alt_shift(KeyCode::Up)],
        ResizePaneAction::Up,
        Some("resize pane"),
        Visibility::Shown,
        Some(glyph::ALT_SHIFT_ALL_ARROWS),
    ),
    KeyBinding::borrowed(
        &[KeyChord::alt_shift(KeyCode::Down)],
        ResizePaneAction::Down,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::alt_shift(KeyCode::Left)],
        ResizePaneAction::Left,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::alt_shift(KeyCode::Right)],
        ResizePaneAction::Right,
        None,
        Visibility::HiddenAlias,
        None,
    ),
];
pub(crate) static RESIZE_PANE_KEYMAP: Keymap<ResizePaneAction> =
    Keymap::from_static(RESIZE_PANE_KEYMAP_BINDINGS);

mod dialogs;
mod prefix;
pub(crate) use dialogs::{
    FILTER_LIST_KEYMAP, FilterListAction, READ_ONLY_DISMISS_KEYMAP, RENAME_KEYMAP,
    ReadOnlyDismissAction, RenameAction,
};
pub(crate) use prefix::PREFIX_COMMAND_KEYMAP;
