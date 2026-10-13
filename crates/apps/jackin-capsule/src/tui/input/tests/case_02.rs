// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
