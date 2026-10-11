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

use crate::tui::input::InputEvent;

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

mod dialogs;
mod prefix;
pub(crate) use dialogs::{
    FILTER_LIST_KEYMAP, FilterListAction, READ_ONLY_DISMISS_KEYMAP, RENAME_KEYMAP,
    ReadOnlyDismissAction, RenameAction,
};
pub(crate) use prefix::PREFIX_COMMAND_KEYMAP;

#[cfg(test)]
mod tests;
