// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings trust-tab keymap.

use termrock::input::KeyCode;

use termrock::keymap::{KeyBinding, KeyChord, Keymap, Visibility};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsTrustTabAction {
    MoveUp,
    MoveDown,
    ScrollLeft,
    ScrollRight,
    Toggle,
    Save,
    /// Caller resolves: if dirty → `ConfirmDiscard`, else `ReturnToList`.
    Back,
}

pub(crate) static SETTINGS_TRUST_TAB_KEYMAP_BINDINGS: &[KeyBinding<SettingsTrustTabAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Up)],
        SettingsTrustTabAction::MoveUp,
        Some("navigate"),
        Visibility::Shown,
        Some("↑↓"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Down)],
        SettingsTrustTabAction::MoveDown,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        SettingsTrustTabAction::MoveUp,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        SettingsTrustTabAction::MoveDown,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('h')),
            KeyChord::plain(KeyCode::Char('H')),
        ],
        SettingsTrustTabAction::ScrollLeft,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('L')),
        ],
        SettingsTrustTabAction::ScrollRight,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char(' '))],
        SettingsTrustTabAction::Toggle,
        Some("trust/untrust"),
        Visibility::Shown,
        Some("␣"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        SettingsTrustTabAction::Save,
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
        SettingsTrustTabAction::Back,
        Some("back"),
        Visibility::Shown,
        Some("Q"),
    ),
];
pub(crate) static SETTINGS_TRUST_TAB_KEYMAP: Keymap<SettingsTrustTabAction> =
    Keymap::from_static(SETTINGS_TRUST_TAB_KEYMAP_BINDINGS);

// ── Settings Global Mounts tab ────────────────────────────────────────────────
