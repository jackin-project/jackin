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

pub(crate) static EDITOR_GLOBAL_KEYMAP: Keymap<EditorGlobalAction> = Keymap::new(&[
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        action: EditorGlobalAction::Save,
        hint: Some("save"),
        visibility: Visibility::Shown,
        glyph: Some("S"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Esc)],
        action: EditorGlobalAction::Escape,
        hint: Some("back / discard"),
        visibility: Visibility::Shown,
        glyph: Some("Esc"),
    },
]);

// ── Editor tab-bar mode ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorTabBarAction {
    PrevTab,
    NextTab,
    FocusContent,
}

pub(crate) static EDITOR_TAB_BAR_KEYMAP: Keymap<EditorTabBarAction> = Keymap::new(&[
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Left),
            KeyChord::plain(KeyCode::BackTab),
        ],
        action: EditorTabBarAction::PrevTab,
        hint: Some("prev tab"),
        visibility: Visibility::Shown,
        glyph: Some("←/⇤"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Right)],
        action: EditorTabBarAction::NextTab,
        hint: Some("next tab"),
        visibility: Visibility::Shown,
        glyph: Some("→"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Tab),
            KeyChord::plain(KeyCode::Down),
        ],
        action: EditorTabBarAction::FocusContent,
        hint: Some("focus content"),
        visibility: Visibility::Shown,
        glyph: Some("⇥/↓"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        action: EditorTabBarAction::FocusContent,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
]);

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

pub(crate) static EDITOR_CONTENT_KEYMAP: Keymap<EditorContentAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Up)],
        action: EditorContentAction::MoveUp,
        hint: Some("move field"),
        visibility: Visibility::Shown,
        glyph: Some("↑↓"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Down)],
        action: EditorContentAction::MoveDown,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        action: EditorContentAction::MoveUp,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        action: EditorContentAction::MoveDown,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('h')),
            KeyChord::plain(KeyCode::Char('H')),
        ],
        action: EditorContentAction::ScrollLeft,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('L')),
        ],
        action: EditorContentAction::ScrollRight,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Left)],
        action: EditorContentAction::CollapseHeader,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Right)],
        action: EditorContentAction::ExpandHeader,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Tab)],
        action: EditorContentAction::NextTab,
        hint: Some("next tab"),
        visibility: Visibility::Shown,
        glyph: Some("⇥"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::BackTab)],
        action: EditorContentAction::FocusTabBar,
        hint: Some("tab bar"),
        visibility: Visibility::Shown,
        glyph: Some("⇤"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Enter)],
        action: EditorContentAction::CheckImmediate,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
]);

// ── Settings tab-bar mode ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsTabBarAction {
    PrevTab,
    NextTab,
    FocusContent,
}

pub(crate) static SETTINGS_TAB_BAR_KEYMAP: Keymap<SettingsTabBarAction> = Keymap::new(&[
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Left),
            KeyChord::plain(KeyCode::BackTab),
        ],
        action: SettingsTabBarAction::PrevTab,
        hint: Some("prev tab"),
        visibility: Visibility::Shown,
        glyph: Some("←/⇤"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Right)],
        action: SettingsTabBarAction::NextTab,
        hint: Some("next tab"),
        visibility: Visibility::Shown,
        glyph: Some("→"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Tab),
            KeyChord::plain(KeyCode::Down),
        ],
        action: SettingsTabBarAction::FocusContent,
        hint: Some("focus content"),
        visibility: Visibility::Shown,
        glyph: Some("⇥/↓"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        action: SettingsTabBarAction::FocusContent,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
]);

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

pub(crate) static SETTINGS_CONTENT_SHELL_KEYMAP: Keymap<SettingsContentShellAction> =
    Keymap::new(&[
        KeyBinding {
            chords: &[KeyChord::plain(KeyCode::Tab)],
            action: SettingsContentShellAction::NextTab,
            hint: Some("next tab"),
            visibility: Visibility::Shown,
            glyph: Some("⇥"),
        },
        KeyBinding {
            chords: &[KeyChord::plain(KeyCode::BackTab)],
            action: SettingsContentShellAction::FocusTabBar,
            hint: Some("tab bar"),
            visibility: Visibility::Shown,
            glyph: Some("⇤"),
        },
        KeyBinding {
            chords: &[KeyChord::plain(KeyCode::Esc)],
            action: SettingsContentShellAction::FocusTabBarOrClearAuth,
            hint: None,
            visibility: Visibility::Internal,
            glyph: None,
        },
    ]);

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

pub(crate) static SETTINGS_GENERAL_TAB_KEYMAP: Keymap<SettingsGeneralTabAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Up)],
        action: SettingsGeneralTabAction::MoveUp,
        hint: Some("navigate"),
        visibility: Visibility::Shown,
        glyph: Some("↑↓"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Down)],
        action: SettingsGeneralTabAction::MoveDown,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        action: SettingsGeneralTabAction::MoveUp,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        action: SettingsGeneralTabAction::MoveDown,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Char(' '))],
        action: SettingsGeneralTabAction::Toggle,
        hint: Some("toggle"),
        visibility: Visibility::Shown,
        glyph: Some("␣"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        action: SettingsGeneralTabAction::Save,
        hint: Some("save"),
        visibility: Visibility::Shown,
        glyph: Some("S"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::plain(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Char('Q')),
        ],
        action: SettingsGeneralTabAction::Back,
        hint: Some("back"),
        visibility: Visibility::Shown,
        glyph: Some("Q"),
    },
]);

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

pub(crate) static SETTINGS_ENV_TAB_KEYMAP: Keymap<SettingsEnvTabAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Up)],
        action: SettingsEnvTabAction::MoveUp,
        hint: Some("navigate"),
        visibility: Visibility::Shown,
        glyph: Some("↑↓"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Down)],
        action: SettingsEnvTabAction::MoveDown,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        action: SettingsEnvTabAction::MoveUp,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        action: SettingsEnvTabAction::MoveDown,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('a')),
            KeyChord::plain(KeyCode::Char('A')),
        ],
        action: SettingsEnvTabAction::Add,
        hint: Some("add"),
        visibility: Visibility::Shown,
        glyph: Some("A"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        action: SettingsEnvTabAction::Save,
        hint: Some("save"),
        visibility: Visibility::Shown,
        glyph: Some("S"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('d')),
            KeyChord::plain(KeyCode::Char('D')),
        ],
        action: SettingsEnvTabAction::Delete,
        hint: Some("delete"),
        visibility: Visibility::Shown,
        glyph: Some("D"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('m')),
            KeyChord::plain(KeyCode::Char('M')),
        ],
        action: SettingsEnvTabAction::ToggleMask,
        hint: Some("mask"),
        visibility: Visibility::Shown,
        glyph: Some("M"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('p')),
            KeyChord::plain(KeyCode::Char('P')),
        ],
        action: SettingsEnvTabAction::OpenPicker,
        hint: Some("op picker"),
        visibility: Visibility::Shown,
        glyph: Some("P"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Enter)],
        action: SettingsEnvTabAction::Enter,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::plain(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Char('Q')),
        ],
        action: SettingsEnvTabAction::Back,
        hint: Some("back"),
        visibility: Visibility::Shown,
        glyph: Some("Q"),
    },
]);

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

pub(crate) static SETTINGS_TRUST_TAB_KEYMAP: Keymap<SettingsTrustTabAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Up)],
        action: SettingsTrustTabAction::MoveUp,
        hint: Some("navigate"),
        visibility: Visibility::Shown,
        glyph: Some("↑↓"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Down)],
        action: SettingsTrustTabAction::MoveDown,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        action: SettingsTrustTabAction::MoveUp,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        action: SettingsTrustTabAction::MoveDown,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('h')),
            KeyChord::plain(KeyCode::Char('H')),
        ],
        action: SettingsTrustTabAction::ScrollLeft,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('L')),
        ],
        action: SettingsTrustTabAction::ScrollRight,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Char(' '))],
        action: SettingsTrustTabAction::Toggle,
        hint: Some("trust/untrust"),
        visibility: Visibility::Shown,
        glyph: Some("␣"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        action: SettingsTrustTabAction::Save,
        hint: Some("save"),
        visibility: Visibility::Shown,
        glyph: Some("S"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::plain(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Char('Q')),
        ],
        action: SettingsTrustTabAction::Back,
        hint: Some("back"),
        visibility: Visibility::Shown,
        glyph: Some("Q"),
    },
]);

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

pub(crate) static SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP: Keymap<SettingsGlobalMountsTabAction> =
    Keymap::new(&[
        KeyBinding {
            chords: &[KeyChord::plain(KeyCode::Up)],
            action: SettingsGlobalMountsTabAction::MoveUp,
            hint: Some("navigate"),
            visibility: Visibility::Shown,
            glyph: Some("↑↓"),
        },
        KeyBinding {
            chords: &[KeyChord::plain(KeyCode::Down)],
            action: SettingsGlobalMountsTabAction::MoveDown,
            hint: None,
            visibility: Visibility::Internal,
            glyph: None,
        },
        KeyBinding {
            chords: &[
                KeyChord::plain(KeyCode::Char('k')),
                KeyChord::plain(KeyCode::Char('K')),
            ],
            action: SettingsGlobalMountsTabAction::MoveUp,
            hint: None,
            visibility: Visibility::HiddenAlias,
            glyph: None,
        },
        KeyBinding {
            chords: &[
                KeyChord::plain(KeyCode::Char('j')),
                KeyChord::plain(KeyCode::Char('J')),
            ],
            action: SettingsGlobalMountsTabAction::MoveDown,
            hint: None,
            visibility: Visibility::HiddenAlias,
            glyph: None,
        },
        KeyBinding {
            chords: &[
                KeyChord::plain(KeyCode::Char('h')),
                KeyChord::plain(KeyCode::Char('H')),
            ],
            action: SettingsGlobalMountsTabAction::ScrollLeft,
            hint: None,
            visibility: Visibility::HiddenAlias,
            glyph: None,
        },
        KeyBinding {
            chords: &[
                KeyChord::plain(KeyCode::Char('l')),
                KeyChord::plain(KeyCode::Char('L')),
            ],
            action: SettingsGlobalMountsTabAction::ScrollRight,
            hint: None,
            visibility: Visibility::HiddenAlias,
            glyph: None,
        },
        KeyBinding {
            chords: &[
                KeyChord::plain(KeyCode::Char('s')),
                KeyChord::plain(KeyCode::Char('S')),
            ],
            action: SettingsGlobalMountsTabAction::Save,
            hint: Some("save"),
            visibility: Visibility::Shown,
            glyph: Some("S"),
        },
        KeyBinding {
            chords: &[
                KeyChord::plain(KeyCode::Char('r')),
                KeyChord::plain(KeyCode::Char('R')),
            ],
            action: SettingsGlobalMountsTabAction::ToggleReadonly,
            hint: Some("readonly"),
            visibility: Visibility::Shown,
            glyph: Some("R"),
        },
        KeyBinding {
            chords: &[
                KeyChord::plain(KeyCode::Char('a')),
                KeyChord::plain(KeyCode::Char('A')),
            ],
            action: SettingsGlobalMountsTabAction::Add,
            hint: Some("add"),
            visibility: Visibility::Shown,
            glyph: Some("A"),
        },
        KeyBinding {
            chords: &[
                KeyChord::plain(KeyCode::Char('d')),
                KeyChord::plain(KeyCode::Char('D')),
            ],
            action: SettingsGlobalMountsTabAction::Delete,
            hint: Some("delete"),
            visibility: Visibility::Shown,
            glyph: Some("D"),
        },
        KeyBinding {
            chords: &[
                KeyChord::plain(KeyCode::Char('o')),
                KeyChord::plain(KeyCode::Char('O')),
            ],
            action: SettingsGlobalMountsTabAction::OpenGithub,
            hint: Some("GitHub"),
            visibility: Visibility::Shown,
            glyph: Some("O"),
        },
        KeyBinding {
            chords: &[
                KeyChord::plain(KeyCode::Char('n')),
                KeyChord::plain(KeyCode::Char('N')),
            ],
            action: SettingsGlobalMountsTabAction::EditRename,
            hint: Some("rename"),
            visibility: Visibility::Shown,
            glyph: Some("N"),
        },
        KeyBinding {
            chords: &[KeyChord::plain(KeyCode::Char('1'))],
            action: SettingsGlobalMountsTabAction::EditSource,
            hint: Some("edit src"),
            visibility: Visibility::Shown,
            glyph: Some("1"),
        },
        KeyBinding {
            chords: &[KeyChord::plain(KeyCode::Char('2'))],
            action: SettingsGlobalMountsTabAction::EditDest,
            hint: Some("edit dst"),
            visibility: Visibility::Shown,
            glyph: Some("2"),
        },
        KeyBinding {
            chords: &[KeyChord::plain(KeyCode::Char('3'))],
            action: SettingsGlobalMountsTabAction::EditScope,
            hint: Some("edit scope"),
            visibility: Visibility::Shown,
            glyph: Some("3"),
        },
        KeyBinding {
            chords: &[KeyChord::plain(KeyCode::Enter)],
            action: SettingsGlobalMountsTabAction::Enter,
            hint: None,
            visibility: Visibility::Internal,
            glyph: None,
        },
        KeyBinding {
            chords: &[
                KeyChord::plain(KeyCode::Esc),
                KeyChord::plain(KeyCode::Char('q')),
                KeyChord::plain(KeyCode::Char('Q')),
            ],
            action: SettingsGlobalMountsTabAction::Back,
            hint: Some("back"),
            visibility: Visibility::Shown,
            glyph: Some("Q"),
        },
    ]);

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

pub(crate) static INLINE_PICKER_SHELL_KEYMAP: Keymap<InlinePickerShellAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Left)],
        action: InlinePickerShellAction::ScrollLeft,
        hint: Some("scroll"),
        visibility: Visibility::Shown,
        glyph: Some("←→"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Right)],
        action: InlinePickerShellAction::ScrollRight,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('h')),
            KeyChord::plain(KeyCode::Char('H')),
        ],
        action: InlinePickerShellAction::ScrollLeft,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('L')),
        ],
        action: InlinePickerShellAction::ScrollRight,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
]);

// ── Row-level hint keymaps (display-only) ─────────────────────────────────────
//
// These keymaps drive hint generation for per-row contextual footer items. They
// are never dispatched — action type is `()`. Each builder function in
// `components/footer_hints.rs` calls `keymap.hint_spans()` instead of
// hard-coding span slices, keeping dispatch and display in sync.

pub(crate) static EDITOR_GENERAL_RENAME_KEYMAP: Keymap<()> = Keymap::new(&[KeyBinding {
    chords: &[KeyChord::plain(KeyCode::Enter)],
    action: (),
    hint: Some("rename"),
    visibility: Visibility::Shown,
    glyph: Some("↵"),
}]);

pub(crate) static EDITOR_GENERAL_WORKDIR_KEYMAP: Keymap<()> = Keymap::new(&[KeyBinding {
    chords: &[KeyChord::plain(KeyCode::Enter)],
    action: (),
    hint: Some("pick working directory"),
    visibility: Visibility::Shown,
    glyph: Some("↵"),
}]);

pub(crate) static EDITOR_GENERAL_TOGGLE_KEYMAP: Keymap<()> = Keymap::new(&[KeyBinding {
    chords: &[KeyChord::plain(KeyCode::Char(' '))],
    action: (),
    hint: Some("toggle"),
    visibility: Visibility::Shown,
    glyph: Some("␣"),
}]);

pub(crate) static EDITOR_ROLE_NEW_KEYMAP: Keymap<()> = Keymap::new(&[KeyBinding {
    chords: &[
        KeyChord::plain(KeyCode::Enter),
        KeyChord::plain(KeyCode::Char('a')),
        KeyChord::plain(KeyCode::Char('A')),
    ],
    action: (),
    hint: Some("load role"),
    visibility: Visibility::Shown,
    glyph: Some("↵/A"),
}]);

pub(crate) static SETTINGS_GENERAL_TOGGLE_KEYMAP: Keymap<()> = Keymap::new(&[KeyBinding {
    chords: &[KeyChord::plain(KeyCode::Char(' '))],
    action: (),
    hint: Some("toggle"),
    visibility: Visibility::Shown,
    glyph: Some("␣"),
}]);

pub(crate) static SETTINGS_TRUST_TOGGLE_KEYMAP: Keymap<()> = Keymap::new(&[KeyBinding {
    chords: &[KeyChord::plain(KeyCode::Char(' '))],
    action: (),
    hint: Some("trust/untrust"),
    visibility: Visibility::Shown,
    glyph: Some("␣"),
}]);

pub(crate) static AUTH_MANAGE_KEYMAP: Keymap<()> = Keymap::new(&[KeyBinding {
    chords: &[KeyChord::plain(KeyCode::Enter)],
    action: (),
    hint: Some("manage auth"),
    visibility: Visibility::Shown,
    glyph: Some("↵"),
}]);

pub(crate) static AUTH_EDIT_SOURCE_KEYMAP: Keymap<()> = Keymap::new(&[KeyBinding {
    chords: &[KeyChord::plain(KeyCode::Enter)],
    action: (),
    hint: Some("edit source"),
    visibility: Visibility::Shown,
    glyph: Some("↵"),
}]);

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
pub(crate) static WORKSPACE_LIST_KEYMAP: Keymap<WorkspaceListAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Up)],
        action: WorkspaceListAction::NavigateUp,
        hint: None,
        visibility: Visibility::Shown,
        glyph: Some("↑↓"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Down)],
        action: WorkspaceListAction::NavigateDown,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        action: WorkspaceListAction::NavigateUp,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        action: WorkspaceListAction::NavigateDown,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Left)],
        action: WorkspaceListAction::TreeLeft,
        hint: None,
        visibility: Visibility::Shown,
        glyph: Some("←"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('h')),
            KeyChord::plain(KeyCode::Char('H')),
        ],
        action: WorkspaceListAction::ScrollLeft,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Right)],
        action: WorkspaceListAction::TreeRight,
        hint: None,
        visibility: Visibility::Shown,
        glyph: Some("→"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('L')),
        ],
        action: WorkspaceListAction::ScrollRight,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Enter)],
        action: WorkspaceListAction::Enter,
        hint: None,
        visibility: Visibility::Shown,
        glyph: Some("↵"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('e')),
            KeyChord::plain(KeyCode::Char('E')),
        ],
        action: WorkspaceListAction::Edit,
        hint: Some("edit"),
        visibility: Visibility::Shown,
        glyph: Some("E"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('n')),
            KeyChord::plain(KeyCode::Char('N')),
        ],
        action: WorkspaceListAction::NewSession,
        hint: None,
        visibility: Visibility::Shown,
        glyph: Some("N"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('d')),
            KeyChord::plain(KeyCode::Char('D')),
        ],
        action: WorkspaceListAction::Delete,
        hint: Some("delete"),
        visibility: Visibility::Shown,
        glyph: Some("D"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('w')),
            KeyChord::plain(KeyCode::Char('W')),
        ],
        action: WorkspaceListAction::Prewarm,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: Some("W"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('o')),
            KeyChord::plain(KeyCode::Char('O')),
        ],
        action: WorkspaceListAction::OpenGithub,
        hint: Some("open in GitHub"),
        visibility: Visibility::Shown,
        glyph: Some("O"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('s')),
            KeyChord::plain(KeyCode::Char('S')),
        ],
        action: WorkspaceListAction::Settings,
        hint: Some("settings"),
        visibility: Visibility::Shown,
        glyph: Some("S"),
    },
    // Instance-row actions. Advertised contextually (instance-row footer only),
    // so they carry no `hint` here and are HiddenAlias for the base hint bar.
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('r')),
            KeyChord::plain(KeyCode::Char('R')),
        ],
        action: WorkspaceListAction::InstanceReconnect,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: Some("R"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('a')),
            KeyChord::plain(KeyCode::Char('A')),
        ],
        action: WorkspaceListAction::InstanceNewSession,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: Some("A"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('x')),
            KeyChord::plain(KeyCode::Char('X')),
        ],
        action: WorkspaceListAction::InstanceShell,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: Some("X"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('i')),
            KeyChord::plain(KeyCode::Char('I')),
        ],
        action: WorkspaceListAction::InstanceInspect,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: Some("I"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('t')),
            KeyChord::plain(KeyCode::Char('T')),
        ],
        action: WorkspaceListAction::InstanceStop,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: Some("T"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('p')),
            KeyChord::plain(KeyCode::Char('P')),
        ],
        action: WorkspaceListAction::ConfirmPurge,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: Some("P"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Tab)],
        action: WorkspaceListAction::EnterPreview,
        hint: Some("into preview"),
        visibility: Visibility::Shown,
        glyph: Some("⇥"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::plain(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Char('Q')),
        ],
        action: WorkspaceListAction::Exit,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    // Ctrl-Q is intercepted upstream by `should_open_quit_confirm`; it never
    // reaches the list resolver (which dispatches modifier-free chords). The
    // binding exists only so the footer can derive the `Ctrl-Q` glyph.
    KeyBinding {
        chords: &[KeyChord::ctrl(KeyCode::Char('q'))],
        action: WorkspaceListAction::Quit,
        hint: Some("quit"),
        visibility: Visibility::Internal,
        glyph: Some("Ctrl-Q"),
    },
]);

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
pub(crate) static PREVIEW_PANE_KEYMAP: Keymap<PreviewPaneAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Up)],
        action: PreviewPaneAction::NavigateUp,
        hint: Some("navigate panes"),
        visibility: Visibility::Shown,
        glyph: Some("↑↓"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Down)],
        action: PreviewPaneAction::NavigateDown,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('K')),
        ],
        action: PreviewPaneAction::NavigateUp,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('J')),
        ],
        action: PreviewPaneAction::NavigateDown,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::Enter)],
        action: PreviewPaneAction::Attach,
        hint: Some("attach focused pane"),
        visibility: Visibility::Shown,
        glyph: Some("↵"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(KeyCode::Esc),
            KeyChord::plain(KeyCode::Left),
        ],
        action: PreviewPaneAction::Back,
        hint: Some("back"),
        visibility: Visibility::Shown,
        glyph: Some("Esc/←"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(KeyCode::BackTab)],
        action: PreviewPaneAction::Back,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
]);

#[cfg(test)]
mod tests;
