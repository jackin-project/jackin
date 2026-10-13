// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    CAPSULE_GLOBAL_KEYMAP, FILTER_LIST_KEYMAP, FilterListAction, GlobalCapsuleAction,
    PREFIX_COMMAND_KEYMAP, READ_ONLY_DISMISS_KEYMAP, RENAME_KEYMAP, RESIZE_PANE_KEYMAP,
    ReadOnlyDismissAction, RenameAction,
};
use crate::tui::input::{ArrowDir, PrefixCommand};
use crate::tui::keymap::raw_bytes_to_chord;
use termrock::input::{KeyChord, KeyCode};

fn assert_shown_glyphs_are_normalized<A: Copy + 'static>(keymap: &termrock::input::Keymap<A>) {
    for span in keymap.hint_spans() {
        let termrock::widgets::HintSpan::Key(key) = span else {
            continue;
        };
        assert_ne!(key, concat!("T", "ab"));
        assert!(!key.contains("\u{2191}/"));
        assert!(!key.contains("\u{2190}/"));
        assert!(!key.contains(concat!("PgUp", " PgDn")));
        assert!(!key.contains(concat!("Alt", "+")));
        assert!(!key.contains(concat!("Shift", "+")));
        assert!(!key.contains(concat!("Ctrl", "+")));
    }
}

#[test]
fn shown_keymap_glyphs_use_canonical_spellings() {
    assert_shown_glyphs_are_normalized(&CAPSULE_GLOBAL_KEYMAP);
    assert_shown_glyphs_are_normalized(&PREFIX_COMMAND_KEYMAP);
    assert_shown_glyphs_are_normalized(&FILTER_LIST_KEYMAP);
    assert_shown_glyphs_are_normalized(&RENAME_KEYMAP);
    assert_shown_glyphs_are_normalized(&READ_ONLY_DISMISS_KEYMAP);
    assert_shown_glyphs_are_normalized(&RESIZE_PANE_KEYMAP);
}

#[test]
fn prefix_keymap_covers_all_prefix_binding_keys() {
    let cases: &[(&[u8], PrefixCommand)] = &[
        (b"c", PrefixCommand::NewTab),
        (b"n", PrefixCommand::NextTab),
        (b"p", PrefixCommand::PrevTab),
        (b"0", PrefixCommand::JumpTab(0)),
        (b"9", PrefixCommand::JumpTab(9)),
        (b"h", PrefixCommand::MoveFocus(ArrowDir::Left)),
        (b"j", PrefixCommand::MoveFocus(ArrowDir::Down)),
        (b"k", PrefixCommand::MoveFocus(ArrowDir::Up)),
        (b"l", PrefixCommand::MoveFocus(ArrowDir::Right)),
        (b"z", PrefixCommand::ZoomToggle),
        (b"x", PrefixCommand::KillPane),
        (b"&", PrefixCommand::KillTab),
        (b"d", PrefixCommand::Detach),
        (b" ", PrefixCommand::Palette),
        (b":", PrefixCommand::Palette),
        (b"r", PrefixCommand::Redraw),
    ];
    for (raw, expected) in cases {
        let chord = raw_bytes_to_chord(raw).unwrap_or_else(|| panic!("no chord for {raw:?}"));
        let action = PREFIX_COMMAND_KEYMAP
            .dispatch(chord)
            .unwrap_or_else(|| panic!("keymap did not dispatch {raw:?} (chord {chord:?})"));
        assert_eq!(action, *expected, "wrong action for {raw:?}");
    }
}

#[test]
fn prefix_keymap_covers_ctrl_l() {
    let chord = raw_bytes_to_chord(&[0x0c]).expect("0x0c → Ctrl-L");
    assert_eq!(chord, KeyChord::ctrl(KeyCode::Char('l')));
    assert_eq!(
        PREFIX_COMMAND_KEYMAP.dispatch(chord),
        Some(PrefixCommand::ClearPane),
        "Ctrl-L must dispatch to ClearPane"
    );
}

#[test]
fn prefix_keymap_has_shown_hints_for_primary_commands() {
    let spans = PREFIX_COMMAND_KEYMAP.hint_spans();
    let keys: Vec<&str> = spans
        .iter()
        .filter_map(|s| match s {
            termrock::widgets::HintSpan::Key(k) => Some(*k),
            _ => None,
        })
        .collect();
    // h advertises all four directions with a grouped glyph; j/k/l are HiddenAlias.
    assert!(
        keys.contains(&"h/j/k/l"),
        "must show grouped nav glyph: {keys:?}"
    );
    assert!(
        !keys.contains(&"h"),
        "h must not appear as solo glyph (grouped with j/k/l): {keys:?}"
    );
    assert!(
        !keys.contains(&"j"),
        "j is HiddenAlias and must not appear in hints: {keys:?}"
    );
    assert!(
        !keys.contains(&"k"),
        "k is HiddenAlias and must not appear in hints: {keys:?}"
    );
    assert!(
        !keys.contains(&"l"),
        "l is HiddenAlias and must not appear in hints: {keys:?}"
    );
    // chord_glyph derives uppercase from Char('c') / Char('x') when glyph: None
    assert!(keys.contains(&"C"), "must show C (new tab): {keys:?}");
    assert!(
        keys.contains(&"X"),
        "must show X (close/kill pane): {keys:?}"
    );
}

#[test]
fn capsule_global_keymap_dispatches_ctrl_q() {
    // Acceptance criterion for roadmap item A.
    let chord = raw_bytes_to_chord(&[0x11]).expect("0x11 → Ctrl-Q chord");
    assert_eq!(chord, KeyChord::ctrl(KeyCode::Char('q')));
    assert_eq!(
        CAPSULE_GLOBAL_KEYMAP.dispatch(chord),
        Some(GlobalCapsuleAction::RequestExit),
        "Ctrl-Q must dispatch to RequestExit via CAPSULE_GLOBAL_KEYMAP"
    );
}

#[test]
fn capsule_global_keymap_hint_spans_include_ctrl_q_quit() {
    let spans = CAPSULE_GLOBAL_KEYMAP.hint_spans();
    let combined: String = spans
        .iter()
        .filter_map(|s| match s {
            termrock::widgets::HintSpan::Key(k) => Some(*k),
            termrock::widgets::HintSpan::Text(t) => Some(*t),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        combined.contains("Ctrl-Q"),
        "global keymap hint must advertise Ctrl-Q: {combined}"
    );
    assert!(
        combined.contains("quit"),
        "global keymap hint must advertise quit label: {combined}"
    );
}

#[test]
fn filter_list_keymap_dispatches_every_advertised_chord() {
    let cases: &[(&[u8], FilterListAction)] = &[
        (b"\x1b[A", FilterListAction::NavigateUp),
        (b"\x1bOA", FilterListAction::NavigateUp),
        (b"\x1b[B", FilterListAction::NavigateDown),
        (b"\x1bOB", FilterListAction::NavigateDown),
        (b"\r", FilterListAction::Confirm),
        (b"\n", FilterListAction::Confirm),
        (b"\x7f", FilterListAction::FilterBackspace),
        (b"\x08", FilterListAction::FilterBackspace),
        (b"\x1b", FilterListAction::Dismiss),
        (b"\x03", FilterListAction::Dismiss),
        (b"\x11", FilterListAction::Dismiss),
    ];
    for (raw, expected) in cases {
        let chord = raw_bytes_to_chord(raw).unwrap_or_else(|| panic!("no chord for {raw:?}"));
        assert_eq!(
            FILTER_LIST_KEYMAP.dispatch(chord),
            Some(*expected),
            "wrong action for {raw:?}"
        );
    }
    // Printable input is not in the table — it falls through to the
    // caller's filter-building path.
    for raw in [&b"q"[..], b"a", b" ", b":"] {
        let chord = raw_bytes_to_chord(raw).unwrap_or_else(|| panic!("no chord for {raw:?}"));
        assert_eq!(
            FILTER_LIST_KEYMAP.dispatch(chord),
            None,
            "printable {raw:?} must not dispatch"
        );
    }
}

#[test]
fn rename_keymap_dispatches_every_advertised_chord() {
    let cases: &[(&[u8], RenameAction)] = &[
        (b"\r", RenameAction::Save),
        (b"\n", RenameAction::Save),
        (b"\x7f", RenameAction::FieldBackspace),
        (b"\x08", RenameAction::FieldBackspace),
        (b"\x1b", RenameAction::Dismiss),
        (b"\x03", RenameAction::Dismiss),
        (b"\x11", RenameAction::Dismiss),
    ];
    for (raw, expected) in cases {
        let chord = raw_bytes_to_chord(raw).unwrap_or_else(|| panic!("no chord for {raw:?}"));
        assert_eq!(
            RENAME_KEYMAP.dispatch(chord),
            Some(*expected),
            "wrong action for {raw:?}"
        );
    }
    for raw in [&b"a"[..], b"q", b" "] {
        let chord = raw_bytes_to_chord(raw).unwrap_or_else(|| panic!("no chord for {raw:?}"));
        assert_eq!(
            RENAME_KEYMAP.dispatch(chord),
            None,
            "printable {raw:?} must not dispatch"
        );
    }
}

#[test]
fn read_only_dismiss_keymap_accepts_historical_dismiss_set() {
    for raw in [
        &b"\x1b"[..], // Esc
        b"q",
        b"Q",
        b"\x03", // Ctrl+C
        b"\x11", // Ctrl+Q
        b"\x7f", // Backspace (DEL)
        b"\x08", // Ctrl+H / older Backspace
    ] {
        let chord = raw_bytes_to_chord(raw).unwrap_or_else(|| panic!("no chord for {raw:?}"));
        assert_eq!(
            READ_ONLY_DISMISS_KEYMAP.dispatch(chord),
            Some(ReadOnlyDismissAction::Dismiss),
            "{raw:?} must dismiss read-only dialog"
        );
    }
    // Non-dismiss keys must not resolve.
    for raw in [&b"a"[..], b"\r", b"\x1b[A"] {
        let chord = raw_bytes_to_chord(raw).unwrap_or_else(|| panic!("no chord for {raw:?}"));
        assert_eq!(
            READ_ONLY_DISMISS_KEYMAP.dispatch(chord),
            None,
            "{raw:?} must not dismiss read-only dialog"
        );
    }
}
