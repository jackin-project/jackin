// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console keymaps — single source of truth coupling key dispatch and hint-bar
//! advertisement for all host-console surfaces.
//!
//! Every keyboard-driven surface (editor, settings tabs, inline picker) defines
//! its keymap here. `Keymap::dispatch(chord)` replaces plan-function calls in
//! `input/*.rs`; `Keymap::hint_spans()` derives footer hints.

use termrock::input::KeyCode;
use termrock::keymap::{KeyBinding, KeyChord, Keymap, Visibility};

// ── Editor global (fired in both tab-bar and content modes) ──────────────────

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

/// Actions resolvable from a key on the workspace-list screen.
///
/// The keymap resolves a key to one of these; `workspace_list_key_plan` then
/// folds in runtime context the table cannot carry (list-scroll focus, the
/// selected row's type) to produce the final `WorkspaceListKeyPlan`. Footer
/// builders pull each advertised key's glyph from this same table via
/// [`crate::tui::components::Keymap::glyph_for`], so an advertised key cannot
/// drift from the dispatched key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceListAction {
    NavigateUp,
    NavigateDown,
    TreeLeft,
    TreeRight,
    ScrollLeft,
    ScrollRight,
    Enter,
    Edit,
    NewSession,
    Delete,
    OpenGithub,
    Settings,
    Prewarm,
    InstanceReconnect,
    InstanceNewSession,
    InstanceShell,
    InstanceInspect,
    InstanceStop,
    ConfirmPurge,
    EnterPreview,
    Exit,
    Quit,
}

/// Authoritative keymap for the workspace list: single source for both
/// `workspace_list_key_plan` dispatch and the workspace-row / instance-row
/// footer glyphs in `components/footer_hints.rs`.
///
/// Hint labels are intentionally absent from most rows here because the same
/// glyph carries different labels per context (`↵` = "launch" on a workspace
/// row, "reconnect" on an instance row; `N` = "new" vs "new session"). Footers
/// supply the contextual label and take only the glyph from this table.
pub(crate) static WORKSPACE_LIST_KEYMAP_BINDINGS: &[KeyBinding<WorkspaceListAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Up)],
        WorkspaceListAction::NavigateUp,
        None,
        Visibility::Shown,
        Some("↑↓"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Down)],
        WorkspaceListAction::NavigateDown,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        WorkspaceListAction::NavigateUp,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        WorkspaceListAction::NavigateDown,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Left)],
        WorkspaceListAction::TreeLeft,
        None,
        Visibility::Shown,
        Some("←"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('h')),
            KeyChord::plain(KeyCode::Char('H')),
        ],
        WorkspaceListAction::ScrollLeft,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Right)],
        WorkspaceListAction::TreeRight,
        None,
        Visibility::Shown,
        Some("→"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('L')),
        ],
        WorkspaceListAction::ScrollRight,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        WorkspaceListAction::Enter,
        None,
        Visibility::Shown,
        Some("↵"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('e')),
            KeyChord::plain(KeyCode::Char('E')),
        ],
        WorkspaceListAction::Edit,
        Some("edit"),
        Visibility::Shown,
        Some("E"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('n')),
            KeyChord::plain(KeyCode::Char('N')),
        ],
        WorkspaceListAction::NewSession,
        None,
        Visibility::Shown,
        Some("N"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('d')),
            KeyChord::plain(KeyCode::Char('D')),
        ],
        WorkspaceListAction::Delete,
        Some("delete"),
        Visibility::Shown,
        Some("D"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('w')),
            KeyChord::plain(KeyCode::Char('W')),
        ],
        WorkspaceListAction::Prewarm,
        None,
        Visibility::HiddenAlias,
        Some("W"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('o')),
            KeyChord::plain(KeyCode::Char('O')),
        ],
        WorkspaceListAction::OpenGithub,
        Some("open in GitHub"),
        Visibility::Shown,
        Some("O"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        WorkspaceListAction::Settings,
        Some("settings"),
        Visibility::Shown,
        Some("S"),
    ),
    // Instance-row actions. Advertised contextually (instance-row footer only),
    // so they carry no `hint` here and are HiddenAlias for the base hint bar.
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('r')),
            KeyChord::plain(KeyCode::Char('R')),
        ],
        WorkspaceListAction::InstanceReconnect,
        None,
        Visibility::HiddenAlias,
        Some("R"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('a')),
            KeyChord::plain(KeyCode::Char('A')),
        ],
        WorkspaceListAction::InstanceNewSession,
        None,
        Visibility::HiddenAlias,
        Some("A"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('x')),
            KeyChord::plain(KeyCode::Char('X')),
        ],
        WorkspaceListAction::InstanceShell,
        None,
        Visibility::HiddenAlias,
        Some("X"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('i')),
            KeyChord::plain(KeyCode::Char('I')),
        ],
        WorkspaceListAction::InstanceInspect,
        None,
        Visibility::HiddenAlias,
        Some("I"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('t')),
            KeyChord::plain(KeyCode::Char('T')),
        ],
        WorkspaceListAction::InstanceStop,
        None,
        Visibility::HiddenAlias,
        Some("T"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('p')),
            KeyChord::plain(KeyCode::Char('P')),
        ],
        WorkspaceListAction::ConfirmPurge,
        None,
        Visibility::HiddenAlias,
        Some("P"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Tab)],
        WorkspaceListAction::EnterPreview,
        Some("into preview"),
        Visibility::Shown,
        Some("⇥"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::plain(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Char('Q')),
        ],
        WorkspaceListAction::Exit,
        None,
        Visibility::Internal,
        None,
    ),
    // Ctrl-Q is intercepted upstream by `should_open_quit_confirm`; it never
    // reaches the list resolver (which dispatches modifier-free chords). The
    // binding exists only so the footer can derive the `Ctrl-Q` glyph.
    KeyBinding::borrowed(
        &[KeyChord::ctrl(KeyCode::Char('q'))],
        WorkspaceListAction::Quit,
        Some("quit"),
        Visibility::Internal,
        Some("Ctrl-Q"),
    ),
];
pub(crate) static WORKSPACE_LIST_KEYMAP: Keymap<WorkspaceListAction> =
    Keymap::from_static(WORKSPACE_LIST_KEYMAP_BINDINGS);

// ── Preview pane (workspace list → preview focus) ─────────────────────────────

/// Actions in the workspace-list preview-pane focus mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreviewPaneAction {
    NavigateUp,
    NavigateDown,
    Attach,
    Back,
}

/// Authoritative keymap for preview-pane focus: drives both
/// `preview_pane_key_plan` dispatch and the `PreviewPane` footer (which is
/// `PREVIEW_PANE_KEYMAP.hint_spans()` verbatim — no context branches).
pub(crate) static PREVIEW_PANE_KEYMAP_BINDINGS: &[KeyBinding<PreviewPaneAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Up)],
        PreviewPaneAction::NavigateUp,
        Some("navigate panes"),
        Visibility::Shown,
        Some("↑↓"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Down)],
        PreviewPaneAction::NavigateDown,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        PreviewPaneAction::NavigateUp,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        PreviewPaneAction::NavigateDown,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Enter)],
        PreviewPaneAction::Attach,
        Some("attach focused pane"),
        Visibility::Shown,
        Some("↵"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::plain(KeyCode::Left),
        ],
        PreviewPaneAction::Back,
        Some("back"),
        Visibility::Shown,
        Some("Esc/←"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::BackTab)],
        PreviewPaneAction::Back,
        None,
        Visibility::HiddenAlias,
        None,
    ),
];
pub(crate) static PREVIEW_PANE_KEYMAP: Keymap<PreviewPaneAction> =
    Keymap::from_static(PREVIEW_PANE_KEYMAP_BINDINGS);

#[cfg(test)]
mod tests;
