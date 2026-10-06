// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor global, tab-bar, and content keymaps.

use termrock::input::KeyCode;

use termrock::keymap::{KeyBinding, KeyChord, Keymap, Visibility};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorGlobalAction {
    Save,
    Escape,
}

pub(crate) static EDITOR_GLOBAL_KEYMAP_BINDINGS: &[KeyBinding<EditorGlobalAction>] = &[
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        EditorGlobalAction::Save,
        Some("save"),
        Visibility::Shown,
        Some("S"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Esc)],
        EditorGlobalAction::Escape,
        Some("back / discard"),
        Visibility::Shown,
        Some("Esc"),
    ),
];
pub(crate) static EDITOR_GLOBAL_KEYMAP: Keymap<EditorGlobalAction> =
    Keymap::from_static(EDITOR_GLOBAL_KEYMAP_BINDINGS);

// ── Editor tab-bar mode ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorTabBarAction {
    PrevTab,
    NextTab,
    FocusContent,
}

pub(crate) static EDITOR_TAB_BAR_KEYMAP_BINDINGS: &[KeyBinding<EditorTabBarAction>] = &[
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Left),
            KeyChord::plain(KeyCode::BackTab),
        ],
        EditorTabBarAction::PrevTab,
        Some("prev tab"),
        Visibility::Shown,
        Some("←/⇤"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Right)],
        EditorTabBarAction::NextTab,
        Some("next tab"),
        Visibility::Shown,
        Some("→"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Tab),
            KeyChord::plain(KeyCode::Down),
        ],
        EditorTabBarAction::FocusContent,
        Some("focus content"),
        Visibility::Shown,
        Some("⇥/↓"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        EditorTabBarAction::FocusContent,
        None,
        Visibility::HiddenAlias,
        None,
    ),
];
pub(crate) static EDITOR_TAB_BAR_KEYMAP: Keymap<EditorTabBarAction> =
    Keymap::from_static(EDITOR_TAB_BAR_KEYMAP_BINDINGS);

// ── Editor content mode ───────────────────────────────────────────────────────

/// Actions for the editor when content (not the tab bar) has focus.
///
/// `Char(_)` wildcard is unrepresentable in a static keymap; the dispatch site
/// in `input/editor.rs` falls through to `CheckImmediateAction` for any `Char`
/// chord not matched here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorContentAction {
    MoveUp,
    MoveDown,
    ScrollLeft,
    ScrollRight,
    ExpandHeader,
    CollapseHeader,
    NextTab,
    FocusTabBar,
    CheckImmediate,
}

pub(crate) static EDITOR_CONTENT_KEYMAP_BINDINGS: &[KeyBinding<EditorContentAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Up)],
        EditorContentAction::MoveUp,
        Some("move field"),
        Visibility::Shown,
        Some("↑↓"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Down)],
        EditorContentAction::MoveDown,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        EditorContentAction::MoveUp,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        EditorContentAction::MoveDown,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('h')),
            KeyChord::plain(KeyCode::Char('H')),
        ],
        EditorContentAction::ScrollLeft,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('L')),
        ],
        EditorContentAction::ScrollRight,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Left)],
        EditorContentAction::CollapseHeader,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Right)],
        EditorContentAction::ExpandHeader,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Tab)],
        EditorContentAction::NextTab,
        Some("next tab"),
        Visibility::Shown,
        Some("⇥"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::BackTab)],
        EditorContentAction::FocusTabBar,
        Some("tab bar"),
        Visibility::Shown,
        Some("⇤"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        EditorContentAction::CheckImmediate,
        None,
        Visibility::Internal,
        None,
    ),
];
pub(crate) static EDITOR_CONTENT_KEYMAP: Keymap<EditorContentAction> =
    Keymap::from_static(EDITOR_CONTENT_KEYMAP_BINDINGS);

// ── Settings tab-bar mode ─────────────────────────────────────────────────────
