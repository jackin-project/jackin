//! Tests for `input`.
use super::*;

fn parse_all_default(input: &[u8]) -> Vec<InputEvent> {
    InputParser::default().parse(input)
}

fn parse_all_prefix_only(input: &[u8]) -> Vec<InputEvent> {
    InputParser::new(Some(0x02), None).parse(input)
}

#[test]
fn ctrl_backslash_opens_palette_by_default() {
    let events = parse_all_default(b"\x1c");
    assert_eq!(events, vec![InputEvent::OpenPalette]);
}

#[test]
fn ctrl_q_requests_exit() {
    // Ctrl+Q (0x11) is the global quit chord — intercepted, not forwarded.
    let events = parse_all_default(b"\x11");
    assert_eq!(events, vec![InputEvent::RequestExit]);
}

#[test]
fn csi_u_ctrl_backslash_opens_palette_by_default() {
    // Two encodings: `\x1b[92;5u` (codepoint=92='\\', Ctrl modifier) and the
    // legacy bare-codepoint form `\x1b[28u` (codepoint=0x1C=Ctrl+\). Both must
    // reach the same palette-open action.
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
    // Shift-only (modifier=2) must not produce a control byte.
    assert_eq!(csi_u_control_byte(u32::from(b'a'), Some(2)), None);
    // No modifier at all must not produce a control byte.
    assert_eq!(csi_u_control_byte(u32::from(b'a'), None), None);
}

#[test]
fn csi_u_uppercase_ctrl_converts_to_control_byte() {
    // Terminals in CSI-u mode encode Ctrl+A (uppercase) as codepoint=65, modifier=5.
    // `csi_u_control_byte` must map that to 0x01 so it can be dispatched against
    // the global keymap — same logic applies to the full A-Z range.
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
    // Ctrl+] (0x1D) is not the palette key and not in the global keymap.
    // `dispatch_control_byte` returns None → classify_csi returns None →
    // the raw bytes are forwarded verbatim as Data.
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
    // Bracketed paste / multi-line input continuation must reach
    // the PTY as `\n` under the default palette binding.
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
    // Bracketed paste protects the palette byte inside paste content.
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
    // CSI-u extended-keys encoding: `\x1b[13;2u` = Shift+Enter.
    let events = parse_all_default(b"\x1b[13;2u");
    match &events[..] {
        [InputEvent::Data(b)] => assert_eq!(b, b"\x1b[13;2u"),
        other => panic!("Shift+Enter must round-trip: {other:?}"),
    }
}

#[test]
fn shift_enter_xterm_modify_other_keys_normalises_to_csi_u() {
    // Ghostty emits xterm modifyOtherKeys before CSI-u/kitty mode
    // is active: `CSI 27 ; 2 ; 13 ~` = Shift+Enter. Codex expects
    // the CSI-u form for multiline input.
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
    // Kitty progressive-enhancement arrow Down press, no modifier:
    // `\x1b[1;1:1B`. The dialog navigator only recognises the
    // legacy `\x1b[B`, so the parser must rewrite the kitty form
    // before the byte sequence reaches Dialog::handle_key — every
    // other arrow direction follows the same rule.
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
    // Event tag 2 (repeat) must reach the dialog / agent so a
    // held-down arrow continues scrolling instead of stalling
    // after the first emit.
    let events = parse_all_default(b"\x1b[1;1:2B");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b[B".to_vec())]);
}

#[test]
fn kitty_arrow_release_is_suppressed() {
    // Event tag 3 (release) must not surface as a Data event.
    // Forwarding it surfaces as a stray `\x1b[1;1:3B` visible at
    // the agent's prompt and confuses TUIs that key off press
    // events. Both the dialog and the agent only ever care about
    // press / repeat.
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
    // Alt+Shift+Arrow stays a multiplexer-level pane-resize gesture
    // even when the outer terminal is in kitty mode — the event
    // tag is parsed, the press is acted on, the release is
    // suppressed (same shape as the no-modifier case above).
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
    // Encoding without an event tag stays untouched — agents that
    // consume the legacy modifier form (Ctrl+Arrow word nav etc.)
    // continue to receive it byte-for-byte.
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
    // Stateful parser must drop an in-progress CSI/OSC/OtherEsc
    // sequence when it exceeds MAX_ESC_SEQ_LEN — otherwise a
    // peer streaming `\x1b[` followed by megabytes of parameter
    // bytes across many parse() calls grows self.seq forever.
    let mut parser = InputParser::default();
    // Open the CSI parameter run.
    parser.parse(b"\x1b[");
    // Feed enough parameter bytes (no final 0x40..=0x7E) to overflow.
    let junk = vec![b';'; MAX_ESC_SEQ_LEN + 256];
    parser.parse(&junk);
    // After the cap fires the parser resets; a well-formed
    // sequence right after must classify cleanly.
    let events = parser.parse(b"\x1b[A");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b[A".to_vec())]);
}

#[test]
fn unterminated_osc_does_not_grow_unbounded() {
    let mut parser = InputParser::default();
    parser.parse(b"\x1b]52;c;");
    let junk = vec![b'A'; MAX_ESC_SEQ_LEN + 256];
    parser.parse(&junk);
    // Cap fires; a fresh sequence resyncs.
    let events = parser.parse(b"\x1b[A");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b[A".to_vec())]);
}

#[test]
fn parse_prefix_forms() {
    assert_eq!(parse_prefix("C-a"), Some(0x01));
    assert_eq!(parse_prefix("C-b"), Some(0x02));
    assert_eq!(parse_prefix("c-z"), Some(0x1A));
    assert_eq!(parse_prefix("0x02"), Some(0x02));
    assert_eq!(parse_prefix("0X1B"), Some(0x1B));
    assert_eq!(parse_prefix("Q"), Some(b'Q'));
    assert_eq!(parse_prefix("nope"), None);
}
