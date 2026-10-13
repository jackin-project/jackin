// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Prefix-mode command keymap: chords dispatched after the prefix key.

use crate::tui::input::{ArrowDir, PrefixCommand};
use termrock::input::{KeyBinding, KeyChord, KeyCode, Keymap, Visibility};

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
