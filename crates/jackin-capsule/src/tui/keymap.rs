//! Capsule keymaps — static binding tables for capsule TUI key dispatch.
//!
//! The capsule's outer input parser (`input.rs`) handles the palette key
//! and prefix key as raw bytes from the PTY (dynamically configured via
//! `JACKIN_PALETTE_KEY` / `JACKIN_PREFIX` env vars). Those dynamic chords
//! cannot live in a static `Keymap`. What IS static is the set of commands
//! that follow the prefix key — those are registered here.

use jackin_tui::components::{KeyBinding, KeyChord, Keymap, LogicalKey, Visibility};

use crate::tui::input::{ArrowDir, PrefixCommand};

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
        hint: Some("new"),
        visibility: Visibility::Shown,
        glyph: Some("c"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('n'))],
        action: PrefixCommand::NextTab,
        hint: Some("next tab"),
        visibility: Visibility::Shown,
        glyph: Some("n"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('x'))],
        action: PrefixCommand::KillPane,
        hint: Some("close"),
        visibility: Visibility::Shown,
        glyph: Some("x"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('h'))],
        action: PrefixCommand::MoveFocus(ArrowDir::Left),
        hint: Some("focus left"),
        visibility: Visibility::Shown,
        glyph: Some("h"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('j'))],
        action: PrefixCommand::MoveFocus(ArrowDir::Down),
        hint: Some("focus down"),
        visibility: Visibility::Shown,
        glyph: Some("j"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('k'))],
        action: PrefixCommand::MoveFocus(ArrowDir::Up),
        hint: Some("focus up"),
        visibility: Visibility::Shown,
        glyph: Some("k"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('l'))],
        action: PrefixCommand::MoveFocus(ArrowDir::Right),
        hint: Some("focus right"),
        visibility: Visibility::Shown,
        glyph: Some("l"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('"'))],
        action: PrefixCommand::SplitTopBottom,
        hint: Some("split top/bottom"),
        visibility: Visibility::Shown,
        glyph: Some("\""),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('%'))],
        action: PrefixCommand::SplitSideBySide,
        hint: Some("split left/right"),
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

#[cfg(test)]
mod tests;
