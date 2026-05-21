/// Prefix-key state machine — regression coverage for the
/// `Ctrl+J = 0x0A` palette bug and surrounding behaviour.
use jackin_container::input::{ArrowDir, InputEvent, InputParser, PrefixCommand};

fn parse(bytes: &[u8]) -> Vec<InputEvent> {
    InputParser::default().parse(bytes)
}

#[test]
fn plain_lf_reaches_pty() {
    let events = parse(b"line1\nline2\n");
    assert_eq!(events, vec![InputEvent::Data(b"line1\nline2\n".to_vec())]);
}

#[test]
fn prefix_only_consumes_no_pty_byte() {
    let events = parse(b"\x02");
    assert!(events.is_empty());
}

#[test]
fn double_prefix_forwards_one_literal_byte_to_pty() {
    let events = parse(b"\x02\x02");
    assert_eq!(events, vec![InputEvent::Data(vec![0x02])]);
}

#[test]
fn prefix_followed_by_unknown_byte_resets_state() {
    // Unknown command after prefix is consumed but emits no event;
    // the parser must return to Idle so subsequent bytes route normally.
    let events = parse(b"\x02!hello");
    assert_eq!(events, vec![InputEvent::Data(b"hello".to_vec())]);
}

#[test]
fn arrow_key_round_trips_without_classification() {
    let events = parse(b"\x1b[A");
    assert_eq!(events, vec![InputEvent::Data(b"\x1b[A".to_vec())]);
}

#[test]
fn focus_in_classified_separately() {
    assert_eq!(parse(b"\x1b[I"), vec![InputEvent::FocusIn]);
    assert_eq!(parse(b"\x1b[O"), vec![InputEvent::FocusOut]);
}

#[test]
fn sgr_mouse_press_emits_mouse_event() {
    assert_eq!(
        parse(b"\x1b[<2;10;5M"),
        vec![InputEvent::MousePress {
            col: 9,
            row: 4,
            button: 2
        }]
    );
}

#[test]
fn bracketed_paste_protects_prefix_byte() {
    let events = parse(b"before\x1b[200~paste\x02inside\n\x1b[201~after");
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
fn prefix_then_arrow_passes_arrow_through_as_unknown() {
    // `prefix + <arrow>` after the lone prefix is a key with no
    // binding in the default table; it returns to Idle without
    // emitting anything (and certainly does not eat the next bytes).
    let events = parse(b"\x02\x1b[Cnext");
    let combined: Vec<u8> = events
        .iter()
        .flat_map(|e| match e {
            InputEvent::Data(b) => b.clone(),
            _ => Vec::new(),
        })
        .collect();
    assert!(
        combined.ends_with(b"next"),
        "next must reach PTY: {events:?}"
    );
}

#[test]
fn prefix_commands_for_default_bindings() {
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
        (b"\x02d", Detach),
        (b"\x02 ", Palette),
        (b"\x02:", Palette),
        (b"\x02r", Redraw),
    ];
    for (input, expected) in bindings {
        let evs = parse(input);
        assert_eq!(evs, vec![InputEvent::PrefixCommand(expected.clone())]);
    }
}

#[test]
fn awaiting_state_observable_between_bytes() {
    let mut p = InputParser::default();
    let _ = p.parse(b"\x02");
    assert!(p.is_awaiting_prefix());
    let _ = p.parse(b"c");
    assert!(!p.is_awaiting_prefix());
}
