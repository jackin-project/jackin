// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::TerminalText;

#[test]
fn sanitize_strips_escape_sequences_and_control_bytes() {
    let cases: [(&[u8], &str); 15] = [
        (b"ok \x1b[31mred\x1b[0m done\n", "ok red done\n"),
        (b"a\x1b[1;38;5;196mb\x1b[?25lc", "abc"),
        (b"a\x1b]0;title\x07b", "ab"),
        (b"a\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\b", "alinkb"),
        (b"a\x1bPq#data\x1b\\b", "ab"),
        (b"a\x1b(Bb\x1bMc", "abc"),
        (b"bell\x07!", "bell!"),
        (b"tab\tnl\nrest\r\nx\ry", "tab\tnl\nrest\nxy"),
        (b"n\0u\x08l\x7fl", "null"),
        (b"bad\xffutf8\xc3", "bad\u{fffd}utf8\u{fffd}"),
        (b"x\x1b[", "x"),
        (b"x\x1b", "x"),
        (b"x\x1b]0;unterminated", "x"),
        (b"", ""),
        ("h\u{e9}llo \u{20ac}".as_bytes(), "h\u{e9}llo \u{20ac}"),
    ];
    for (input, expected) in cases {
        assert_eq!(
            TerminalText::sanitize(input).as_str(),
            expected,
            "{input:?}"
        );
    }
}

fn is_clean(text: &str) -> bool {
    !text.contains('\x1b')
        && text
            .chars()
            .all(|c| !c.is_control() || c == '\n' || c == '\t')
}

#[test]
fn sanitize_output_contains_no_control_characters() {
    for byte in 0..=255_u8 {
        let single = TerminalText::sanitize(&[byte]);
        assert!(is_clean(single.as_str()), "byte {byte:#x}");
        let escaped = TerminalText::sanitize(&[0x1b, byte]);
        assert!(is_clean(escaped.as_str()), "esc {byte:#x}");
        let csi = TerminalText::sanitize(&[0x1b, b'[', byte, b'm']);
        assert!(is_clean(csi.as_str()), "csi {byte:#x}");
    }
}
