// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn ctrl_backslash_opens_palette_by_default() {
    let events = parse_all_default(b"\x1c");
    assert_eq!(events, vec![InputEvent::OpenPalette]);
}

#[test]
fn ctrl_q_requests_exit() {
    let events = parse_all_default(b"\x11");
    assert_eq!(events, vec![InputEvent::RequestExit]);
}

#[test]
fn csi_u_ctrl_backslash_opens_palette_by_default() {
    let events = parse_all_default(b"\x1b[92;5u");
    assert_eq!(events, vec![InputEvent::OpenPalette]);
    let events = parse_all_default(b"\x1b[28u");
    assert_eq!(events, vec![InputEvent::OpenPalette]);
}

#[test]
fn csi_u_ctrl_q_requests_exit() {
    let events = parse_all_default(b"\x1b[113;5u");
    assert_eq!(events, vec![InputEvent::RequestExit]);
}

#[test]
fn csi_u_control_release_is_suppressed() {
    let events = parse_all_default(b"\x1b[92;5:3u");
    assert!(events.is_empty(), "release must be dropped: {events:?}");
}

#[test]
fn csi_u_lowercase_ctrl_converts_to_control_byte() {
    assert_eq!(csi_u_control_byte(u32::from(b'a'), Some(5)), Some(0x01));
    assert_eq!(csi_u_control_byte(u32::from(b'z'), Some(5)), Some(0x1A));
}

#[test]
fn csi_u_no_ctrl_modifier_returns_none() {
    assert_eq!(csi_u_control_byte(u32::from(b'a'), Some(2)), None);

    assert_eq!(csi_u_control_byte(u32::from(b'a'), None), None);
}

#[test]
fn csi_u_uppercase_ctrl_converts_to_control_byte() {
    assert_eq!(csi_u_control_byte(u32::from(b'A'), Some(5)), Some(0x01));
    assert_eq!(csi_u_control_byte(u32::from(b'Z'), Some(5)), Some(0x1A));
}

#[test]
fn csi_u_special_chars_ctrl_convert_to_control_bytes() {
    assert_eq!(csi_u_control_byte(u32::from(b'\\'), Some(5)), Some(0x1C));
    assert_eq!(csi_u_control_byte(u32::from(b']'), Some(5)), Some(0x1D));
    assert_eq!(csi_u_control_byte(u32::from(b'^'), Some(5)), Some(0x1E));
    assert_eq!(csi_u_control_byte(u32::from(b'_'), Some(5)), Some(0x1F));
}

#[test]
fn csi_u_unbound_ctrl_byte_falls_through_as_data() {
    let events = parse_all_default(b"\x1b[93;5u");
    match &events[..] {
        [InputEvent::Data(_)] => {}
        other => panic!("unbound ctrl byte must fall through as Data: {other:?}"),
    }
}

#[test]
fn xterm_modify_other_keys_global_shortcuts_are_intercepted() {
    let events = parse_all_default(b"\x1b[27;5;92~");
    assert_eq!(events, vec![InputEvent::OpenPalette]);
    let events = parse_all_default(b"\x1b[27;5;113~");
    assert_eq!(events, vec![InputEvent::RequestExit]);
}

#[test]
fn lone_lf_passes_through_with_default_palette_key() {
    let events = parse_all_default(b"\n");
    assert_eq!(events, vec![InputEvent::Data(b"\n".to_vec())]);
}

#[test]
fn palette_key_disabled_lets_ctrl_backslash_through() {
    let events = InputParser::new(None, None).parse(b"\x1c");
    assert_eq!(events, vec![InputEvent::Data(b"\x1c".to_vec())]);
}

#[test]
fn pasted_text_with_palette_key_does_not_open_palette() {
    let mut parser = InputParser::default();
    let events = parser.parse(b"\x1b[200~hello\x1cworld\x1c\x1b[201~");
    let opens = events
        .iter()
        .filter(|e| matches!(e, InputEvent::OpenPalette))
        .count();
    assert_eq!(opens, 0, "palette must not open inside bracketed paste");
}

#[test]
fn lone_prefix_is_consumed_when_prefix_enabled() {
    let events = parse_all_prefix_only(b"\x02");
    assert!(
        events.is_empty(),
        "lone prefix must not emit any event: {events:?}"
    );
}

#[test]
fn double_prefix_forwards_one_literal() {
    let events = parse_all_prefix_only(b"\x02\x02");
    assert_eq!(events, vec![InputEvent::Data(vec![0x02])]);
}

#[test]
fn prefix_c_opens_new_tab() {
    let events = parse_all_prefix_only(b"\x02c");
    assert_eq!(
        events,
        vec![InputEvent::PrefixCommand(PrefixCommand::NewTab)]
    );
}

#[test]
fn prefix_space_opens_palette() {
    let events = parse_all_prefix_only(b"\x02 ");
    assert_eq!(
        events,
        vec![InputEvent::PrefixCommand(PrefixCommand::Palette)]
    );
}

#[test]
fn prefix_d_detaches() {
    let events = parse_all_prefix_only(b"\x02d");
    assert_eq!(
        events,
        vec![InputEvent::PrefixCommand(PrefixCommand::Detach)]
    );
}

#[test]
fn prefix_u_opens_usage() {
    let events = parse_all_prefix_only(b"\x02u");
    assert_eq!(
        events,
        vec![InputEvent::PrefixCommand(PrefixCommand::Usage)]
    );
}

#[test]
fn bracketed_paste_contents_are_forwarded_with_markers() {
    let mut parser = InputParser::new(Some(0x02), None);
    let mut events = parser.parse(b"\x1b[200~hello\x02world\n\x1b[201~");
    events.retain(|e| !matches!(e, InputEvent::Data(b) if b.is_empty()));
    let combined: Vec<u8> = events
        .iter()
        .flat_map(|e| match e {
            InputEvent::Data(b) => b.clone(),
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(combined, b"\x1b[200~hello\x02world\n\x1b[201~");
}

#[test]
fn arrow_key_csi_passes_through() {
    let events = parse_all_default(b"\x1b[A");
    match &events[..] {
        [InputEvent::Data(b)] => assert_eq!(b, b"\x1b[A"),
        other => panic!("unexpected events {other:?}"),
    }
}

#[test]
fn shift_enter_csi_u_round_trips() {
    let events = parse_all_default(b"\x1b[13;2u");
    match &events[..] {
        [InputEvent::Data(b)] => assert_eq!(b, b"\x1b[13;2u"),
        other => panic!("Shift+Enter must round-trip: {other:?}"),
    }
}

#[test]
fn shift_enter_xterm_modify_other_keys_normalises_to_csi_u() {
    let events = parse_all_default(b"\x1b[27;2;13~");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b[13;2u".to_vec())]);
}

#[test]
fn non_enter_xterm_modify_other_keys_round_trips() {
    let events = parse_all_default(b"\x1b[27;2;65~");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b[27;2;65~".to_vec())]);
}

#[test]
fn kitty_escape_press_normalises_to_bare_esc() {
    assert_eq!(
        parse_all_default(b"\x1b[27u"),
        vec![InputEvent::Data(b"\x1b".to_vec())]
    );
    assert_eq!(
        parse_all_default(b"\x1b[27;1u"),
        vec![InputEvent::Data(b"\x1b".to_vec())]
    );
    assert_eq!(
        parse_all_default(b"\x1b[27;1:1u"),
        vec![InputEvent::Data(b"\x1b".to_vec())]
    );
    assert_eq!(
        parse_all_default(b"\x1b[27;1:2u"),
        vec![InputEvent::Data(b"\x1b".to_vec())]
    );
}

#[test]
fn kitty_escape_release_is_suppressed() {
    let events = parse_all_default(b"\x1b[27;1:3u");
    assert!(
        events.is_empty(),
        "kitty Esc release must be dropped, got {events:?}"
    );
}

#[test]
fn kitty_printable_release_is_suppressed() {
    let events = parse_all_default(b"\x1b[116;1:3u");
    assert!(
        events.is_empty(),
        "kitty printable release must be dropped, got {events:?}"
    );
}

#[test]
fn kitty_arrow_press_normalises_to_legacy_form() {
    let events = parse_all_default(b"\x1b[1;1:1B");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b[B".to_vec())]);
    let events = parse_all_default(b"\x1b[1;1:1A");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b[A".to_vec())]);
    let events = parse_all_default(b"\x1b[1;1:1C");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b[C".to_vec())]);
    let events = parse_all_default(b"\x1b[1;1:1D");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b[D".to_vec())]);
}

#[test]
fn kitty_arrow_repeat_is_treated_as_press() {
    let events = parse_all_default(b"\x1b[1;1:2B");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b[B".to_vec())]);
}

#[test]
fn kitty_arrow_release_is_suppressed() {
    let events = parse_all_default(b"\x1b[1;1:3B");
    assert!(
        events.is_empty(),
        "kitty arrow release must be dropped, got {events:?}"
    );
    let events = parse_all_default(b"\x1b[1;1:3A");
    assert!(events.is_empty());
}

#[test]
fn kitty_alt_shift_arrow_is_resize_pane() {
    let events = parse_all_default(b"\x1b[1;4:1B");
    assert_eq!(events, vec![InputEvent::ResizePane(ArrowDir::Down)]);
    let events = parse_all_default(b"\x1b[1;4:3B");
    assert!(
        events.is_empty(),
        "kitty alt+shift arrow release must be dropped, got {events:?}"
    );
}

#[test]
fn legacy_xterm_modifier_arrow_still_round_trips() {
    let events = parse_all_default(b"\x1b[1;5A");
    match &events[..] {
        [InputEvent::Data(b)] => assert_eq!(b, b"\x1b[1;5A"),
        other => panic!("Ctrl+Up must round-trip: {other:?}"),
    }
}

#[test]
fn focus_event_is_classified() {
    let events = parse_all_default(b"\x1b[I");
    assert_eq!(events, vec![InputEvent::FocusIn]);
    let events = parse_all_default(b"\x1b[O");
    assert_eq!(events, vec![InputEvent::FocusOut]);
}

#[test]
fn xterm_window_report_replies_are_suppressed() {
    let mut parser = InputParser::default();
    assert!(
        parser
            .parse(b"pre")
            .contains(&InputEvent::Data(b"pre".to_vec()))
    );
    assert!(
        parser.parse(b"\x1b[8;37").is_empty(),
        "split CSI report must wait for its final byte"
    );
    assert!(
        parser.parse(b";127t").is_empty(),
        "xterm text-area size reply must not reach the PTY"
    );

    let events = parse_all_default(b"a\x1b[4;1443;2210tb\x1b[6;18;9tc");
    assert_eq!(
        events,
        vec![
            InputEvent::Data(b"a".to_vec()),
            InputEvent::Data(b"b".to_vec()),
            InputEvent::Data(b"c".to_vec()),
        ]
    );
}

#[test]
fn sgr_mouse_press_is_decoded() {
    let events = parse_all_default(b"\x1b[<0;5;3M");
    assert_eq!(
        events,
        vec![InputEvent::MousePress {
            col: 4,
            row: 2,
            button: 0
        }]
    );
}

#[test]
fn unterminated_csi_does_not_grow_unbounded() {
    let mut parser = InputParser::default();

    parser.parse(b"\x1b[");

    let junk = vec![b';'; MAX_ESC_SEQ_LEN + 256];
    parser.parse(&junk);

    let events = parser.parse(b"\x1b[A");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b[A".to_vec())]);
}
