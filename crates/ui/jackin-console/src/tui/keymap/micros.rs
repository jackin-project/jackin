// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Inline-picker and single-action micro keymaps.

use termrock::input::KeyCode;

use termrock::keymap::{KeyBinding, KeyChord, Keymap, Visibility};

/// Actions in the inline picker shell wrapping `SelectListState`.
///
/// `q/Q` exit is omitted: both callers unified to `exit_on_q = false`
/// (q filters, quit via Ctrl+Q).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InlinePickerShellAction {
    ScrollLeft,
    ScrollRight,
}

pub(crate) static INLINE_PICKER_SHELL_KEYMAP_BINDINGS: &[KeyBinding<InlinePickerShellAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Left)],
        InlinePickerShellAction::ScrollLeft,
        Some("scroll"),
        Visibility::Shown,
        Some("←→"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Right)],
        InlinePickerShellAction::ScrollRight,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('h')),
            KeyChord::plain(KeyCode::Char('H')),
        ],
        InlinePickerShellAction::ScrollLeft,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('L')),
        ],
        InlinePickerShellAction::ScrollRight,
        None,
        Visibility::HiddenAlias,
        None,
    ),
];
pub(crate) static INLINE_PICKER_SHELL_KEYMAP: Keymap<InlinePickerShellAction> =
    Keymap::from_static(INLINE_PICKER_SHELL_KEYMAP_BINDINGS);

// ── Row-level hint keymaps (display-only) ─────────────────────────────────────
//
// These keymaps drive hint generation for per-row contextual footer items. They
// are never dispatched — action type is `()`. Each builder function in
// `components/footer_hints.rs` calls `keymap.hint_spans()` instead of
// hard-coding span slices, keeping dispatch and display in sync.

pub(crate) static EDITOR_GENERAL_RENAME_KEYMAP_BINDINGS: &[KeyBinding<()>] =
    &[KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        (),
        Some("rename"),
        Visibility::Shown,
        Some("↵"),
    )];
pub(crate) static EDITOR_GENERAL_RENAME_KEYMAP: Keymap<()> =
    Keymap::from_static(EDITOR_GENERAL_RENAME_KEYMAP_BINDINGS);

pub(crate) static EDITOR_GENERAL_WORKDIR_KEYMAP_BINDINGS: &[KeyBinding<()>] =
    &[KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        (),
        Some("pick working directory"),
        Visibility::Shown,
        Some("↵"),
    )];
pub(crate) static EDITOR_GENERAL_WORKDIR_KEYMAP: Keymap<()> =
    Keymap::from_static(EDITOR_GENERAL_WORKDIR_KEYMAP_BINDINGS);

pub(crate) static EDITOR_GENERAL_TOGGLE_KEYMAP_BINDINGS: &[KeyBinding<()>] =
    &[KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char(' '))],
        (),
        Some("toggle"),
        Visibility::Shown,
        Some("␣"),
    )];
pub(crate) static EDITOR_GENERAL_TOGGLE_KEYMAP: Keymap<()> =
    Keymap::from_static(EDITOR_GENERAL_TOGGLE_KEYMAP_BINDINGS);

pub(crate) static EDITOR_ROLE_NEW_KEYMAP_BINDINGS: &[KeyBinding<()>] = &[KeyBinding::borrowed(
    &[
        KeyChord::plain(KeyCode::Enter),
        KeyChord::plain(KeyCode::Char('a')),
        KeyChord::plain(KeyCode::Char('A')),
    ],
    (),
    Some("load role"),
    Visibility::Shown,
    Some("↵/A"),
)];
pub(crate) static EDITOR_ROLE_NEW_KEYMAP: Keymap<()> =
    Keymap::from_static(EDITOR_ROLE_NEW_KEYMAP_BINDINGS);

pub(crate) static SETTINGS_GENERAL_TOGGLE_KEYMAP_BINDINGS: &[KeyBinding<()>] =
    &[KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char(' '))],
        (),
        Some("toggle"),
        Visibility::Shown,
        Some("␣"),
    )];
pub(crate) static SETTINGS_GENERAL_TOGGLE_KEYMAP: Keymap<()> =
    Keymap::from_static(SETTINGS_GENERAL_TOGGLE_KEYMAP_BINDINGS);

pub(crate) static SETTINGS_TRUST_TOGGLE_KEYMAP_BINDINGS: &[KeyBinding<()>] =
    &[KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char(' '))],
        (),
        Some("trust/untrust"),
        Visibility::Shown,
        Some("␣"),
    )];
pub(crate) static SETTINGS_TRUST_TOGGLE_KEYMAP: Keymap<()> =
    Keymap::from_static(SETTINGS_TRUST_TOGGLE_KEYMAP_BINDINGS);

pub(crate) static AUTH_MANAGE_KEYMAP_BINDINGS: &[KeyBinding<()>] = &[KeyBinding::borrowed(
    &[KeyChord::plain(KeyCode::Enter)],
    (),
    Some("manage auth"),
    Visibility::Shown,
    Some("↵"),
)];
pub(crate) static AUTH_MANAGE_KEYMAP: Keymap<()> = Keymap::from_static(AUTH_MANAGE_KEYMAP_BINDINGS);

pub(crate) static AUTH_EDIT_SOURCE_KEYMAP_BINDINGS: &[KeyBinding<()>] = &[KeyBinding::borrowed(
    &[KeyChord::plain(KeyCode::Enter)],
    (),
    Some("edit source"),
    Visibility::Shown,
    Some("↵"),
)];
pub(crate) static AUTH_EDIT_SOURCE_KEYMAP: Keymap<()> =
    Keymap::from_static(AUTH_EDIT_SOURCE_KEYMAP_BINDINGS);

// ── Workspace list ────────────────────────────────────────────────────────────
