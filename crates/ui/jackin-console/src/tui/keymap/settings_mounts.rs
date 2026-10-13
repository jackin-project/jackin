// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings global-mounts-tab keymap.

use termrock::input::KeyCode;

use termrock::keymap::{KeyBinding, KeyChord, Keymap, Visibility};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsGlobalMountsTabAction {
    MoveUp,
    MoveDown,
    ScrollLeft,
    ScrollRight,
    /// s/S — caller checks `has_sensitive_mount` to route `ConfirmSensitiveSave` vs `OpenSavePreview`.
    Save,
    ToggleReadonly,
    /// a/A — always Add; Enter on the add-row also → Add, checked by caller.
    Add,
    /// d/D — caller checks `mount_count` > 0.
    Delete,
    OpenGithub,
    EditRename,
    EditSource,
    EditDest,
    EditScope,
    /// Enter — fires when Enter pressed; caller routes to Add (if `add_row_selected`) else `Noop`.
    Enter,
    /// Caller resolves: if dirty → `ConfirmDiscard`, else `ReturnToList`.
    Back,
}

pub(crate) static SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP_BINDINGS: &[KeyBinding<
    SettingsGlobalMountsTabAction,
>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Up)],
        SettingsGlobalMountsTabAction::MoveUp,
        Some("navigate"),
        Visibility::Shown,
        Some("↑↓"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Down)],
        SettingsGlobalMountsTabAction::MoveDown,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        SettingsGlobalMountsTabAction::MoveUp,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        SettingsGlobalMountsTabAction::MoveDown,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('h')),
            KeyChord::plain(KeyCode::Char('H')),
        ],
        SettingsGlobalMountsTabAction::ScrollLeft,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('L')),
        ],
        SettingsGlobalMountsTabAction::ScrollRight,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        SettingsGlobalMountsTabAction::Save,
        Some("save"),
        Visibility::Shown,
        Some("S"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('r')),
            KeyChord::plain(KeyCode::Char('R')),
        ],
        SettingsGlobalMountsTabAction::ToggleReadonly,
        Some("readonly"),
        Visibility::Shown,
        Some("R"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('a')),
            KeyChord::plain(KeyCode::Char('A')),
        ],
        SettingsGlobalMountsTabAction::Add,
        Some("add"),
        Visibility::Shown,
        Some("A"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('d')),
            KeyChord::plain(KeyCode::Char('D')),
        ],
        SettingsGlobalMountsTabAction::Delete,
        Some("delete"),
        Visibility::Shown,
        Some("D"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('o')),
            KeyChord::plain(KeyCode::Char('O')),
        ],
        SettingsGlobalMountsTabAction::OpenGithub,
        Some("GitHub"),
        Visibility::Shown,
        Some("O"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('n')),
            KeyChord::plain(KeyCode::Char('N')),
        ],
        SettingsGlobalMountsTabAction::EditRename,
        Some("rename"),
        Visibility::Shown,
        Some("N"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('1'))],
        SettingsGlobalMountsTabAction::EditSource,
        Some("edit src"),
        Visibility::Shown,
        Some("1"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('2'))],
        SettingsGlobalMountsTabAction::EditDest,
        Some("edit dst"),
        Visibility::Shown,
        Some("2"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('3'))],
        SettingsGlobalMountsTabAction::EditScope,
        Some("edit scope"),
        Visibility::Shown,
        Some("3"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        SettingsGlobalMountsTabAction::Enter,
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
        SettingsGlobalMountsTabAction::Back,
        Some("back"),
        Visibility::Shown,
        Some("Q"),
    ),
];
pub(crate) static SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP: Keymap<SettingsGlobalMountsTabAction> =
    Keymap::from_static(SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP_BINDINGS);

// ── Inline picker shell ───────────────────────────────────────────────────────
