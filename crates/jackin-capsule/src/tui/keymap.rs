// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule keymaps — static binding tables for capsule TUI key dispatch.
//!
//! The capsule's outer input parser (`input.rs`) handles the palette key
//! and prefix key as raw bytes from the PTY (dynamically configured via
//! `JACKIN_PALETTE_KEY` / `JACKIN_PREFIX` env vars). Those dynamic chords
//! cannot live in a static `Keymap`. What IS static is the set of commands
//! that follow the prefix key — those are registered here.

use termrock::input::{KeyBinding, KeyChord, Keymap, LogicalKey, Visibility};
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
pub(crate) static CAPSULE_GLOBAL_KEYMAP: Keymap<GlobalCapsuleAction> = Keymap::new(&[KeyBinding {
    chords: &[KeyChord::ctrl(LogicalKey::Char('q'))],
    action: GlobalCapsuleAction::RequestExit,
    hint: Some("quit"),
    visibility: Visibility::Shown,
    glyph: None, // auto-derives "Ctrl-Q"
}]);

/// Static binding table for prefix-mode commands.
///
/// After the prefix key is consumed, the next keystroke is looked up here.
/// This table drives both `prefix_binding` dispatch and the prefix cheat-sheet
/// in `main_view_hint` (shown when `prefix_awaiting == true`).
///
/// Palette toggle (`space`/`:`) is included as `Internal` — it's redundant
/// when already in prefix mode (operator can always dismiss and open palette),
/// but listed for dispatch completeness.
pub(crate) static PREFIX_COMMAND_KEYMAP: Keymap<PrefixCommand> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('c'))],
        action: PrefixCommand::NewTab,
        hint: Some("new tab"),
        visibility: Visibility::Shown,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('n'))],
        action: PrefixCommand::NextTab,
        hint: Some("next tab"),
        visibility: Visibility::Shown,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('x'))],
        action: PrefixCommand::KillPane,
        hint: Some("close"),
        visibility: Visibility::Shown,
        glyph: None,
    },
    // h — primary focus nav; grouped glyph advertises all four directions.
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('h'))],
        action: PrefixCommand::MoveFocus(ArrowDir::Left),
        hint: Some("nav"),
        visibility: Visibility::Shown,
        glyph: Some("h/j/k/l"),
    },
    // j, k, l — dispatch but do not produce hint spans.
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('j'))],
        action: PrefixCommand::MoveFocus(ArrowDir::Down),
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('k'))],
        action: PrefixCommand::MoveFocus(ArrowDir::Up),
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('l'))],
        action: PrefixCommand::MoveFocus(ArrowDir::Right),
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('"'))],
        action: PrefixCommand::SplitTopBottom,
        hint: Some("split ↕"),
        visibility: Visibility::Shown,
        glyph: Some("\""),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('%'))],
        action: PrefixCommand::SplitSideBySide,
        hint: Some("split ↔"),
        visibility: Visibility::Shown,
        glyph: Some("%"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('z'))],
        action: PrefixCommand::ZoomToggle,
        hint: Some("zoom"),
        visibility: Visibility::Shown,
        glyph: Some("z"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('p'))],
        action: PrefixCommand::PrevTab,
        hint: Some("prev tab"),
        visibility: Visibility::Shown,
        glyph: Some("p"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('&'))],
        action: PrefixCommand::KillTab,
        hint: Some("kill tab"),
        visibility: Visibility::Shown,
        glyph: Some("&"),
    },
    KeyBinding {
        chords: &[KeyChord::ctrl(LogicalKey::Char('l'))],
        action: PrefixCommand::ClearPane,
        hint: Some("clear"),
        visibility: Visibility::Shown,
        glyph: Some("Ctrl-L"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('d'))],
        action: PrefixCommand::Detach,
        hint: Some("detach"),
        visibility: Visibility::Shown,
        glyph: Some("d"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('u'))],
        action: PrefixCommand::Usage,
        hint: Some("usage"),
        visibility: Visibility::Shown,
        glyph: Some("u"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char(' ')),
            KeyChord::plain(LogicalKey::Char(':')),
        ],
        action: PrefixCommand::Palette,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('r'))],
        action: PrefixCommand::Redraw,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    // JumpTab 0-9 — register as Internal since full list is not hint-bar-friendly
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('0'))],
        action: PrefixCommand::JumpTab(0),
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('1'))],
        action: PrefixCommand::JumpTab(1),
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('2'))],
        action: PrefixCommand::JumpTab(2),
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('3'))],
        action: PrefixCommand::JumpTab(3),
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('4'))],
        action: PrefixCommand::JumpTab(4),
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('5'))],
        action: PrefixCommand::JumpTab(5),
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('6'))],
        action: PrefixCommand::JumpTab(6),
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('7'))],
        action: PrefixCommand::JumpTab(7),
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('8'))],
        action: PrefixCommand::JumpTab(8),
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('9'))],
        action: PrefixCommand::JumpTab(9),
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
]);

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

pub(crate) static FILTER_LIST_KEYMAP: Keymap<FilterListAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Up)],
        action: FilterListAction::NavigateUp,
        hint: Some("navigate"),
        visibility: Visibility::Shown,
        glyph: Some("↑↓"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Down)],
        action: FilterListAction::NavigateDown,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Enter)],
        action: FilterListAction::Confirm,
        hint: Some("select"),
        visibility: Visibility::Shown,
        glyph: Some("↵"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Backspace)],
        action: FilterListAction::FilterBackspace,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Esc),
            KeyChord::ctrl(LogicalKey::Char('c')),
            KeyChord::ctrl(LogicalKey::Char('q')),
        ],
        action: FilterListAction::Dismiss,
        hint: Some("cancel"),
        visibility: Visibility::Shown,
        glyph: Some("Ctrl-C/Esc"),
    },
]);

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

pub(crate) static RENAME_KEYMAP: Keymap<RenameAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Enter)],
        action: RenameAction::Save,
        hint: Some("save"),
        visibility: Visibility::Shown,
        glyph: Some("↵"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Backspace)],
        action: RenameAction::FieldBackspace,
        hint: None,
        visibility: Visibility::Internal,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Esc),
            KeyChord::ctrl(LogicalKey::Char('c')),
            KeyChord::ctrl(LogicalKey::Char('q')),
        ],
        action: RenameAction::Dismiss,
        hint: Some("cancel"),
        visibility: Visibility::Shown,
        glyph: Some("Ctrl-C/Esc"),
    },
]);

// ── Dialog: read-only dismiss ─────────────────────────────────────────────────

/// Single dismiss action for the read-only info dialogs (`ContainerInfo`,
/// `GitHubContext`).
///
/// The accept-set mirrors the historical `is_dismiss_key`: Esc, `q`/`Q`,
/// Ctrl+C, Ctrl+Q, and Backspace (DEL `0x7f` / Ctrl+H `0x08`, both mapped to
/// `LogicalKey::Backspace`). The advertised glyph stays `"q/Esc"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadOnlyDismissAction {
    Dismiss,
}

pub(crate) static READ_ONLY_DISMISS_KEYMAP: Keymap<ReadOnlyDismissAction> =
    Keymap::new(&[KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Esc),
            KeyChord::plain(LogicalKey::Char('q')),
            KeyChord::plain(LogicalKey::Char('Q')),
            KeyChord::ctrl(LogicalKey::Char('c')),
            KeyChord::ctrl(LogicalKey::Char('q')),
            KeyChord::plain(LogicalKey::Backspace),
        ],
        action: ReadOnlyDismissAction::Dismiss,
        hint: Some("dismiss"),
        visibility: Visibility::Shown,
        glyph: Some("q/Esc"),
    }]);

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
pub(crate) static RESIZE_PANE_KEYMAP: Keymap<ResizePaneAction> = Keymap::new(&[
    KeyBinding {
        chords: &[KeyChord::alt_shift(LogicalKey::Up)],
        action: ResizePaneAction::Up,
        hint: Some("resize pane"),
        visibility: Visibility::Shown,
        glyph: Some(glyph::ALT_SHIFT_ALL_ARROWS),
    },
    KeyBinding {
        chords: &[KeyChord::alt_shift(LogicalKey::Down)],
        action: ResizePaneAction::Down,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::alt_shift(LogicalKey::Left)],
        action: ResizePaneAction::Left,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::alt_shift(LogicalKey::Right)],
        action: ResizePaneAction::Right,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
]);

#[cfg(test)]
mod tests;
