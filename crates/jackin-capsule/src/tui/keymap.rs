// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule keymaps — static binding tables for capsule TUI key dispatch.
//!
//! The capsule's outer input parser (`input.rs`) handles the palette key
//! and prefix key as raw bytes from the PTY (dynamically configured via
//! `JACKIN_PALETTE_KEY` / `JACKIN_PREFIX` env vars). Those dynamic chords
//! cannot live in a static `Keymap`. What IS static is the set of commands
//! that follow the prefix key — those are registered here.

use termrock::input::{KeyBinding, KeyChord, KeyCode, Keymap, Visibility};
use termrock::keymap::glyph;

/// Decode Capsule's raw terminal bytes into the neutral logical key contract.
pub(crate) fn raw_bytes_to_chord(bytes: &[u8]) -> Option<KeyChord> {
    termrock::keymap::raw_bytes_to_chord(bytes)
}

use crate::tui::input::{ArrowDir, InputEvent, PrefixCommand};

// ── Global capsule shortcuts ──────────────────────────────────────────────────

/// Actions available everywhere in the capsule TUI regardless of which dialog
/// or mode is active. These bindings back both dispatch and hint advertisement
/// from a single source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalCapsuleAction {
    RequestExit,
}

impl GlobalCapsuleAction {
    pub(crate) fn to_input_event(self) -> InputEvent {
        match self {
            GlobalCapsuleAction::RequestExit => InputEvent::RequestExit,
        }
    }
}

/// Global keymap for capsule-wide shortcuts. Dispatched before any modal or
/// prefix check so these chords work on every surface without per-mode wiring.
pub(crate) static CAPSULE_GLOBAL_KEYMAP_BINDINGS: &[KeyBinding<GlobalCapsuleAction>] = &[
    // The default glyph auto-derives as "Ctrl-Q".
    KeyBinding::borrowed(
        &[KeyChord::ctrl(KeyCode::Char('q'))],
        GlobalCapsuleAction::RequestExit,
        Some("quit"),
        Visibility::Shown,
        None,
    ),
];
pub(crate) static CAPSULE_GLOBAL_KEYMAP: Keymap<GlobalCapsuleAction> =
    Keymap::from_static(CAPSULE_GLOBAL_KEYMAP_BINDINGS);

/// Static binding table for prefix-mode commands.
///
/// After the prefix key is consumed, the next keystroke is looked up here.
/// This table drives both `prefix_binding` dispatch and the prefix cheat-sheet
/// in `main_view_hint` (shown when `prefix_awaiting == true`).
///
/// Palette toggle (`space`/`:`) is included as `Internal` — it's redundant
/// when already in prefix mode (operator can always dismiss and open palette),
/// but listed for dispatch completeness.
pub(crate) static PREFIX_COMMAND_KEYMAP_BINDINGS: &[KeyBinding<PrefixCommand>] = &[
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('c'))],
        PrefixCommand::NewTab,
        Some("new tab"),
        Visibility::Shown,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('n'))],
        PrefixCommand::NextTab,
        Some("next tab"),
        Visibility::Shown,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('x'))],
        PrefixCommand::KillPane,
        Some("close"),
        Visibility::Shown,
        None,
    ),
    // h — primary focus nav; grouped glyph advertises all four directions.
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('h'))],
        PrefixCommand::MoveFocus(ArrowDir::Left),
        Some("nav"),
        Visibility::Shown,
        Some("h/j/k/l"),
    ),
    // j, k, l — dispatch but do not produce hint spans.
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('j'))],
        PrefixCommand::MoveFocus(ArrowDir::Down),
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('k'))],
        PrefixCommand::MoveFocus(ArrowDir::Up),
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('l'))],
        PrefixCommand::MoveFocus(ArrowDir::Right),
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('"'))],
        PrefixCommand::SplitTopBottom,
        Some("split ↕"),
        Visibility::Shown,
        Some("\""),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('%'))],
        PrefixCommand::SplitSideBySide,
        Some("split ↔"),
        Visibility::Shown,
        Some("%"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('z'))],
        PrefixCommand::ZoomToggle,
        Some("zoom"),
        Visibility::Shown,
        Some("z"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('p'))],
        PrefixCommand::PrevTab,
        Some("prev tab"),
        Visibility::Shown,
        Some("p"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('&'))],
        PrefixCommand::KillTab,
        Some("kill tab"),
        Visibility::Shown,
        Some("&"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::ctrl(KeyCode::Char('l'))],
        PrefixCommand::ClearPane,
        Some("clear"),
        Visibility::Shown,
        Some("Ctrl-L"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('d'))],
        PrefixCommand::Detach,
        Some("detach"),
        Visibility::Shown,
        Some("d"),
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('u'))],
        PrefixCommand::Usage,
        Some("usage"),
        Visibility::Shown,
        Some("u"),
    ),
    KeyBinding::borrowed(
        &[
            KeyChord::plain(KeyCode::Char(' ')),
            KeyChord::plain(KeyCode::Char(':')),
        ],
        PrefixCommand::Palette,
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('r'))],
        PrefixCommand::Redraw,
        None,
        Visibility::Internal,
        None,
    ),
    // JumpTab 0-9 — register as Internal since full list is not hint-bar-friendly
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('0'))],
        PrefixCommand::JumpTab(0),
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('1'))],
        PrefixCommand::JumpTab(1),
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('2'))],
        PrefixCommand::JumpTab(2),
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('3'))],
        PrefixCommand::JumpTab(3),
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('4'))],
        PrefixCommand::JumpTab(4),
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('5'))],
        PrefixCommand::JumpTab(5),
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('6'))],
        PrefixCommand::JumpTab(6),
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('7'))],
        PrefixCommand::JumpTab(7),
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('8'))],
        PrefixCommand::JumpTab(8),
        None,
        Visibility::Internal,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::plain(KeyCode::Char('9'))],
        PrefixCommand::JumpTab(9),
        None,
        Visibility::Internal,
        None,
    ),
];
pub(crate) static PREFIX_COMMAND_KEYMAP: Keymap<PrefixCommand> =
    Keymap::from_static(PREFIX_COMMAND_KEYMAP_BINDINGS);

// ── Dialog: filterable list ───────────────────────────────────────────────────

/// Actions for the type-to-filter list dialogs (command palette, agent picker,
/// close-target picker, split-direction picker, provider picker).
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

// ── Normal mode: pane resize ─────────────────────────────────────────────────

/// Actions for the main view's Alt-Shift-Arrow pane-resize bindings.
///
/// Each variant corresponds to one direction; the `Up` binding carries the
/// shared grouped glyph so the hint bar shows a single
/// entry for all four directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResizePaneAction {
    Up,
    Down,
    Left,
    Right,
}

impl ResizePaneAction {
    pub(crate) fn to_input_event(self) -> InputEvent {
        use crate::tui::input::ArrowDir;
        match self {
            Self::Up => InputEvent::ResizePane(ArrowDir::Up),
            Self::Down => InputEvent::ResizePane(ArrowDir::Down),
            Self::Left => InputEvent::ResizePane(ArrowDir::Left),
            Self::Right => InputEvent::ResizePane(ArrowDir::Right),
        }
    }
}

/// Keymap for the multiplexer's Alt-Shift-Arrow pane-resize shortcut.
///
/// The `Up` binding is [`Visibility::Shown`] with a grouped glyph covering
/// all four directions; `Down`, `Left`, and `Right` are
/// [`Visibility::HiddenAlias`] so they dispatch without duplicating the hint.
/// [`crate::tui::components::dialog::hint`] derives the resize-pane entry from
/// this keymap, keeping dispatch and hint advertisement in sync.
pub(crate) static RESIZE_PANE_KEYMAP_BINDINGS: &[KeyBinding<ResizePaneAction>] = &[
    KeyBinding::borrowed(
        &[KeyChord::alt_shift(KeyCode::Up)],
        ResizePaneAction::Up,
        Some("resize pane"),
        Visibility::Shown,
        Some(glyph::ALT_SHIFT_ALL_ARROWS),
    ),
    KeyBinding::borrowed(
        &[KeyChord::alt_shift(KeyCode::Down)],
        ResizePaneAction::Down,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::alt_shift(KeyCode::Left)],
        ResizePaneAction::Left,
        None,
        Visibility::HiddenAlias,
        None,
    ),
    KeyBinding::borrowed(
        &[KeyChord::alt_shift(KeyCode::Right)],
        ResizePaneAction::Right,
        None,
        Visibility::HiddenAlias,
        None,
    ),
];
pub(crate) static RESIZE_PANE_KEYMAP: Keymap<ResizePaneAction> =
    Keymap::from_static(RESIZE_PANE_KEYMAP_BINDINGS);

#[cfg(test)]
mod tests;
