// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use termrock::input::{KeyBinding, KeyChord, KeyCode, Keymap, Visibility};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FilterListAction {
    NavigateUp,
    NavigateDown,
    Confirm,
    FilterBackspace,
    Dismiss,
}

pub(crate) static FILTER_LIST_KEYMAP_BINDINGS: &[KeyBinding<FilterListAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Up)],
        FilterListAction::NavigateUp,
        Some("navigate"),
        Visibility::Shown,
        Some("↑↓"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Down)],
        FilterListAction::NavigateDown,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        FilterListAction::Confirm,
        Some("select"),
        Visibility::Shown,
        Some("↵"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Backspace)],
        FilterListAction::FilterBackspace,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::ctrl(KeyCode::Char('c')),
            KeyChord::ctrl(KeyCode::Char('q')),
        ],
        FilterListAction::Dismiss,
        Some("cancel"),
        Visibility::Shown,
        Some("Ctrl-C/Esc"),
    ),
];
pub(crate) static FILTER_LIST_KEYMAP: Keymap<FilterListAction> =
    Keymap::from_static(FILTER_LIST_KEYMAP_BINDINGS);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenameAction {
    Save,
    FieldBackspace,
    Dismiss,
}

pub(crate) static RENAME_KEYMAP_BINDINGS: &[KeyBinding<RenameAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        RenameAction::Save,
        Some("save"),
        Visibility::Shown,
        Some("↵"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Backspace)],
        RenameAction::FieldBackspace,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::ctrl(KeyCode::Char('c')),
            KeyChord::ctrl(KeyCode::Char('q')),
        ],
        RenameAction::Dismiss,
        Some("cancel"),
        Visibility::Shown,
        Some("Ctrl-C/Esc"),
    ),
];
pub(crate) static RENAME_KEYMAP: Keymap<RenameAction> = Keymap::from_static(RENAME_KEYMAP_BINDINGS);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadOnlyDismissAction {
    Dismiss,
}

pub(crate) static READ_ONLY_DISMISS_KEYMAP_BINDINGS: &[KeyBinding<ReadOnlyDismissAction>] =
    &[KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::plain(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Char('Q')),
            KeyChord::ctrl(KeyCode::Char('c')),
            KeyChord::ctrl(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Backspace),
        ],
        ReadOnlyDismissAction::Dismiss,
        Some("dismiss"),
        Visibility::Shown,
        Some("q/Esc"),
    )];
pub(crate) static READ_ONLY_DISMISS_KEYMAP: Keymap<ReadOnlyDismissAction> =
    Keymap::from_static(READ_ONLY_DISMISS_KEYMAP_BINDINGS);
