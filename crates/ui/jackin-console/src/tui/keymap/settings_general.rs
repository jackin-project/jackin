// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings general-tab keymap.

use termrock::input::KeyCode;

use termrock::keymap::{KeyBinding, KeyChord, Keymap, Visibility};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsGeneralTabAction {
    MoveUp,
    MoveDown,
    Toggle,
    Save,
    /// Caller resolves: if dirty → `ConfirmDiscard`, else `ReturnToList`.
    Back,
}

pub(crate) static SETTINGS_GENERAL_TAB_KEYMAP_BINDINGS: &[KeyBinding<SettingsGeneralTabAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Up)],
        SettingsGeneralTabAction::MoveUp,
        Some("navigate"),
        Visibility::Shown,
        Some("↑↓"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Down)],
        SettingsGeneralTabAction::MoveDown,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        SettingsGeneralTabAction::MoveUp,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        SettingsGeneralTabAction::MoveDown,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char(' '))],
        SettingsGeneralTabAction::Toggle,
        Some("toggle"),
        Visibility::Shown,
        Some("␣"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        SettingsGeneralTabAction::Save,
        Some("save"),
        Visibility::Shown,
        Some("S"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::plain(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Char('Q')),
        ],
        SettingsGeneralTabAction::Back,
        Some("back"),
        Visibility::Shown,
        Some("Q"),
    ),
];
pub(crate) static SETTINGS_GENERAL_TAB_KEYMAP: Keymap<SettingsGeneralTabAction> =
    Keymap::from_static(SETTINGS_GENERAL_TAB_KEYMAP_BINDINGS);

// ── Settings Env tab ──────────────────────────────────────────────────────────
