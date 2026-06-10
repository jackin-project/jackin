use super::*;

#[test]
fn parses_xterm_four_digit_replies() {
    let buf = b"\x1b]10;rgb:e6e6/e6e6/e6e6\x1b\\\x1b]11;rgb:1717/1717/1717\x07";
    let parsed = extract_color_replies(buf);
    assert_eq!(parsed.fg, Some((0xe6, 0xe6, 0xe6)));
    assert_eq!(parsed.bg, Some((0x17, 0x17, 0x17)));
    assert!(
        parsed.leftover_input.is_empty(),
        "leftover: {:?}",
        parsed.leftover_input
    );
}

#[test]
fn parses_short_channels_and_hash_form() {
    let parsed = extract_color_replies(b"\x1b]11;rgb:f/0/8\x07");
    assert_eq!(parsed.bg, Some((0xff, 0x00, 0x88)));
    let parsed = extract_color_replies(b"\x1b]11;#336699\x07");
    assert_eq!(parsed.bg, Some((0x33, 0x66, 0x99)));
}

#[test]
fn keystrokes_around_replies_survive_in_order() {
    let parsed = extract_color_replies(b"ab\x1b]11;rgb:0000/0000/0000\x07cd");
    assert_eq!(parsed.bg, Some((0, 0, 0)));
    assert_eq!(parsed.leftover_input, b"abcd");
}

#[test]
fn partial_reply_tail_is_withheld_from_leftover() {
    // The reply is split across reads; the partial tail must not leak into
    // forwarded input.
    let parsed = extract_color_replies(b"x\x1b]11;rgb:12");
    assert_eq!(parsed.bg, None);
    assert_eq!(parsed.leftover_input, b"x");
}

#[test]
fn unrelated_osc_one_passes_through() {
    let buf = b"\x1b]1;icon\x07";
    let parsed = extract_color_replies(buf);
    assert_eq!((parsed.fg, parsed.bg), (None, None));
    assert_eq!(parsed.leftover_input, buf.as_slice());
}

#[test]
fn malformed_payload_yields_none() {
    let parsed = extract_color_replies(b"\x1b]11;rgb:zz/00/00\x07");
    assert_eq!(parsed.bg, None);
    let parsed = extract_color_replies(b"\x1b]11;rgb:0/0\x07");
    assert_eq!(parsed.bg, None);
    let parsed = extract_color_replies(b"\x1b]11;rgb:0/0/0/0\x07");
    assert_eq!(parsed.bg, None);
}
