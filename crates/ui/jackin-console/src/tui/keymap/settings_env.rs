// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings env-tab keymap.

use termrock::input::KeyCode;

use termrock::keymap::{KeyBinding, KeyChord, Keymap, Visibility};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsEnvTabAction {
    MoveUp,
    MoveDown,
    Add,
    Save,
    /// d/D — only fires when plain modifier; caller checks context.
    Delete,
    /// m/M — only fires when plain modifier; caller checks context.
    ToggleMask,
    /// p/P — caller checks plain modifier + `op_available`.
    OpenPicker,
    /// Enter — caller routes: if `selected_is_op_ref` && `op_available` → `OpenPicker`, else `OpenEnterModal`.
    Enter,
    /// Caller resolves: if dirty → `ConfirmDiscard`, else `ReturnToList`.
    Back,
}

pub(crate) static SETTINGS_ENV_TAB_KEYMAP_BINDINGS: &[KeyBinding<SettingsEnvTabAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Up)],
        SettingsEnvTabAction::MoveUp,
        Some("navigate"),
        Visibility::Shown,
        Some("↑↓"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Down)],
        SettingsEnvTabAction::MoveDown,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        SettingsEnvTabAction::MoveUp,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        SettingsEnvTabAction::MoveDown,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('a')),
            KeyChord::plain(KeyCode::Char('A')),
        ],
        SettingsEnvTabAction::Add,
        Some("add"),
        Visibility::Shown,
        Some("A"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        SettingsEnvTabAction::Save,
        Some("save"),
        Visibility::Shown,
        Some("S"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('d')),
            KeyChord::plain(KeyCode::Char('D')),
        ],
        SettingsEnvTabAction::Delete,
        Some("delete"),
        Visibility::Shown,
        Some("D"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('m')),
            KeyChord::plain(KeyCode::Char('M')),
        ],
        SettingsEnvTabAction::ToggleMask,
        Some("mask"),
        Visibility::Shown,
        Some("M"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('p')),
            KeyChord::plain(KeyCode::Char('P')),
        ],
        SettingsEnvTabAction::OpenPicker,
        Some("op picker"),
        Visibility::Shown,
        Some("P"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        SettingsEnvTabAction::Enter,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::plain(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Char('Q')),
        ],
        SettingsEnvTabAction::Back,
        Some("back"),
        Visibility::Shown,
        Some("Q"),
    ),
];
pub(crate) static SETTINGS_ENV_TAB_KEYMAP: Keymap<SettingsEnvTabAction> =
    Keymap::from_static(SETTINGS_ENV_TAB_KEYMAP_BINDINGS);

// ── Settings Trust tab ────────────────────────────────────────────────────────
