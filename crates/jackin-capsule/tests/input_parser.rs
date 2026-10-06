// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// Input parser regressions.
///
/// Two parallel models: default `Ctrl+\` palette key; opt-in Ctrl+B
/// prefix mode. Tests exercise both.
use jackin_capsule::tui::input::{ArrowDir, InputEvent, InputParser, PrefixCommand};

fn parse_default(bytes: &[u8]) -> Vec<InputEvent> {
    InputParser::default().parse(bytes)
}

fn parse_prefix_only(bytes: &[u8]) -> Vec<InputEvent> {
    InputParser::new(Some(0x02), None).parse(bytes)
}

#[test]
fn ctrl_backslash_opens_palette_in_default_mode() {
    let events = parse_default(b"\x1c");
    assert_eq!(events, vec![InputEvent::OpenPalette]);
}

#[test]
fn lone_lf_reaches_pty_in_default_mode() {
    // Multi-line input continuation (`\n`) must reach the agent —
    // the previous `Ctrl+J` default ate every LF and broke Claude
    // Code / Codex multi-line entry.
    let events = parse_default(b"\n");
    assert_eq!(events, vec![InputEvent::Data(b"\n".to_vec())]);
}

#[test]
fn palette_key_disabled_lets_ctrl_backslash_through() {
    let events = InputParser::new(None, None).parse(b"\x1c");
    assert_eq!(events, vec![InputEvent::Data(b"\x1c".to_vec())]);
}

#[test]
fn lone_prefix_consumes_no_pty_byte() {
    let events = parse_prefix_only(b"\x02");
    assert!(events.is_empty());
}

#[test]
fn double_prefix_forwards_one_literal_byte_to_pty() {
    let events = parse_prefix_only(b"\x02\x02");
    assert_eq!(events, vec![InputEvent::Data(vec![0x02])]);
}

#[test]
fn prefix_followed_by_unknown_byte_resets_state() {
    let events = parse_prefix_only(b"\x02!hello");
    assert_eq!(events, vec![InputEvent::Data(b"hello".to_vec())]);
}

#[test]
fn arrow_key_round_trips_without_classification() {
    let events = parse_default(b"\x1b[A");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b[A".to_vec())]);
}

#[test]
fn focus_in_classified_separately() {
    assert_eq!(parse_default(b"\x1b[I"), vec![InputEvent::FocusIn]);
    assert_eq!(parse_default(b"\x1b[O"), vec![InputEvent::FocusOut]);
}

#[test]
fn sgr_mouse_press_emits_mouse_event() {
    assert_eq!(
        parse_default(b"\x1b[<2;10;5M"),
        vec![InputEvent::MousePress {
            col: 9,
            row: 4,
            button: 2
        }]
    );
}

#[test]
fn x10_mouse_press_emits_mouse_event() {
    assert_eq!(
        parse_default(b"\x1b[M *%"),
        vec![InputEvent::MousePress {
            col: 9,
            row: 4,
            button: 0
        }]
    );
}

#[test]
fn x10_mouse_release_emits_mouse_event() {
    assert_eq!(
        parse_default(b"\x1b[M#*%"),
        vec![InputEvent::MouseRelease {
            col: 9,
            row: 4,
            button: 0
        }]
    );
}

#[test]
fn x10_mouse_sequence_survives_chunk_boundary() {
    let mut parser = InputParser::default();
    assert!(parser.parse(b"\x1b[M").is_empty());
    assert_eq!(
        parser.parse(b" *%"),
        vec![InputEvent::MousePress {
            col: 9,
            row: 4,
            button: 0
        }]
    );
}

#[test]
fn x10_no_button_motion_from_live_log_is_not_split_into_data() {
    assert_eq!(
        parse_default(b"\x1b[MC~9"),
        vec![InputEvent::MousePress {
            col: 93,
            row: 24,
            button: 35
        }]
    );
}

#[test]
fn bracketed_paste_protects_palette_byte() {
    // Pasted text containing the palette byte must NOT open the palette.
    let mut parser = InputParser::default();
    let events = parser.parse(b"before\x1b[200~hello\x1cworld\x1c\x1b[201~after");
    let opens = events
        .iter()
        .filter(|e| matches!(e, InputEvent::OpenPalette))
        .count();
    assert_eq!(opens, 0, "palette must not open inside bracketed paste");
    let combined: Vec<u8> = events
        .iter()
        .flat_map(|e| match e {
            InputEvent::Data(b) => b.clone(),
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(combined, b"before\x1b[200~hello\x1cworld\x1c\x1b[201~after");
}

#[test]
fn bracketed_paste_protects_prefix_byte() {
    let events = parse_prefix_only(b"before\x1b[200~paste\x02inside\n\x1b[201~after");
    let combined: Vec<u8> = events
        .iter()
        .flat_map(|e| match e {
            InputEvent::Data(b) => b.clone(),
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(combined, b"before\x1b[200~paste\x02inside\n\x1b[201~after");
}

#[test]
fn prefix_commands_for_default_bindings_when_prefix_enabled() {
    use PrefixCommand::*;
    let bindings: &[(&[u8], PrefixCommand)] = &[
        (b"\x02c", NewTab),
        (b"\x02n", NextTab),
        (b"\x02p", PrevTab),
        (b"\x025", JumpTab(5)),
        (b"\x02\"", SplitTopBottom),
        (b"\x02%", SplitSideBySide),
        (b"\x02h", MoveFocus(ArrowDir::Left)),
        (b"\x02j", MoveFocus(ArrowDir::Down)),
        (b"\x02k", MoveFocus(ArrowDir::Up)),
        (b"\x02l", MoveFocus(ArrowDir::Right)),
        (b"\x02z", ZoomToggle),
        (b"\x02x", KillPane),
        (b"\x02&", KillTab),
        (b"\x02\x0c", ClearPane),
        (b"\x02d", Detach),
        (b"\x02 ", Palette),
        (b"\x02:", Palette),
        (b"\x02r", Redraw),
    ];
    for (input, expected) in bindings {
        let evs = parse_prefix_only(input);
        assert_eq!(evs, vec![InputEvent::PrefixCommand(*expected)]);
    }
}

#[test]
fn plain_ctrl_l_reaches_pty_but_prefix_ctrl_l_clears_pane() {
    assert_eq!(parse_default(b"\x0c"), vec![InputEvent::Data(vec![0x0c])]);
    assert_eq!(
        parse_prefix_only(b"\x02\x0c"),
        vec![InputEvent::PrefixCommand(PrefixCommand::ClearPane)]
    );
}

#[test]
fn awaiting_state_observable_between_bytes_with_prefix() {
    let mut p = InputParser::new(Some(0x02), None);
    drop(p.parse(b"\x02"));
    assert!(p.is_awaiting_prefix());
    drop(p.parse(b"c"));
    assert!(!p.is_awaiting_prefix());
}
