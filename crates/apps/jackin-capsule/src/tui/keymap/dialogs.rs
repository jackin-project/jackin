// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Dialog-scoped keymaps: filter list, rename, and read-only dismiss.

use termrock::input::{KeyBinding, KeyChord, KeyCode, Keymap, Visibility};

// ── Dialog: filterable list ───────────────────────────────────────────────────

/// Actions for the type-to-filter list dialogs (command palette, agent picker,
/// close-target picker, split-direction picker, agent picker).
///
/// Printable `Char` input is intentionally absent from the table — it builds the
/// filter and is handled by the dispatch site's `printable_filter_char`
/// fallthrough (the `None` arm), exactly like the editor's `CheckImmediate`
/// wildcard. The differing hint *labels* ("select" vs "launch") and the
/// presence/absence of the "type filter" text live at the hint-builder call
/// site; only the key glyphs derive from this table.
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

// ── Dialog: rename tab ────────────────────────────────────────────────────────

/// Actions for the rename-tab text-input dialog.
///
/// Printable `Char` input is absent — it falls through (the `None` arm) to
/// canonical text-input insertion. Backspace is `Internal`: it edits the field rather
/// than being advertised.
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

// ── Dialog: read-only dismiss ─────────────────────────────────────────────────

/// Single dismiss action for the read-only info dialogs (`ContainerInfo`,
/// `GitHubContext`).
///
/// The accept-set mirrors the historical `is_dismiss_key`: Esc, `q`/`Q`,
/// Ctrl+C, Ctrl+Q, and Backspace (DEL `0x7f` / Ctrl+H `0x08`, both mapped to
/// `KeyCode::Backspace`). The advertised glyph stays `"q/Esc"`.
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
