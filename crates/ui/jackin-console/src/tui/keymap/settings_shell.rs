// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings tab-bar and content-shell keymaps.

use termrock::input::KeyCode;

use termrock::keymap::{KeyBinding, KeyChord, Keymap, Visibility};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsTabBarAction {
    PrevTab,
    NextTab,
    FocusContent,
}

pub(crate) static SETTINGS_TAB_BAR_KEYMAP_BINDINGS: &[KeyBinding<SettingsTabBarAction>] = &[
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Left),
            KeyChord::plain(KeyCode::BackTab),
        ],
        SettingsTabBarAction::PrevTab,
        Some("prev tab"),
        Visibility::Shown,
        Some("←/⇤"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Right)],
        SettingsTabBarAction::NextTab,
        Some("next tab"),
        Visibility::Shown,
        Some("→"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Tab),
            KeyChord::plain(KeyCode::Down),
        ],
        SettingsTabBarAction::FocusContent,
        Some("focus content"),
        Visibility::Shown,
        Some("⇥/↓"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        SettingsTabBarAction::FocusContent,
        None,
        Visibility::HiddenAlias,
        None,
    ),
];
pub(crate) static SETTINGS_TAB_BAR_KEYMAP: Keymap<SettingsTabBarAction> =
    Keymap::from_static(SETTINGS_TAB_BAR_KEYMAP_BINDINGS);

// ── Settings content-shell mode ───────────────────────────────────────────────

/// Shell-level actions when settings content has focus (tab navigation / focus
/// return). Applied before per-tab dispatch in `handle_settings_key_with_effects`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsContentShellAction {
    /// Tab → next tab (focus stays on tab bar after move).
    NextTab,
    /// `BackTab` → return focus to tab bar, no auth-kind clear.
    FocusTabBar,
    /// Esc → return focus to tab bar; caller clears auth kind if one is selected.
    FocusTabBarOrClearAuth,
}

pub(crate) static SETTINGS_CONTENT_SHELL_KEYMAP_BINDINGS: &[KeyBinding<
    SettingsContentShellAction,
>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Tab)],
        SettingsContentShellAction::NextTab,
        Some("next tab"),
        Visibility::Shown,
        Some("⇥"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::BackTab)],
        SettingsContentShellAction::FocusTabBar,
        Some("tab bar"),
        Visibility::Shown,
        Some("⇤"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Esc)],
        SettingsContentShellAction::FocusTabBarOrClearAuth,
        None,
        Visibility::Internal,
        None,
    ),
];
pub(crate) static SETTINGS_CONTENT_SHELL_KEYMAP: Keymap<SettingsContentShellAction> =
    Keymap::from_static(SETTINGS_CONTENT_SHELL_KEYMAP_BINDINGS);

// ── Settings General tab ──────────────────────────────────────────────────────
