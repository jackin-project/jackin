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

const BIDI: [char; 12] = [
    '\u{061c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}',
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
];

fn is_clean(text: &str) -> bool {
    !text.contains('\x1b')
        && text
            .chars()
            .all(|c| (!c.is_control() || c == '\n' || c == '\t') && !BIDI.contains(&c))
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
    for bidi in BIDI {
        let alone = TerminalText::sanitize(bidi.to_string().as_bytes());
        assert!(is_clean(alone.as_str()), "{bidi:?}");
        let escaped = TerminalText::sanitize(format!("\x1b{bidi}").as_bytes());
        assert!(is_clean(escaped.as_str()), "esc {bidi:?}");
    }
}

#[test]
fn unterminated_string_sequences_end_at_the_line_and_bidi_controls_are_stripped() {
    let cases: [(&str, &str); 9] = [
        ("a\x1b]0;title\nline2", "a\nline2"),
        ("a\x1bPq#data\nnext", "a\nnext"),
        ("a\x1b_x\nb", "a\nb"),
        ("a\x1b^x\nb", "a\nb"),
        ("a\x1bXx\nb", "a\nb"),
        ("a\x1b]0;t\r\nb", "a\nb"),
        (
            "hook failed\x1b]0;oops\nERROR: boom\n",
            "hook failed\nERROR: boom\n",
        ),
        ("a\x1b]0;t\x1b\nb", "a\nb"),
        ("x\x1b]0;unterminated", "x"),
    ];
    for (input, expected) in cases {
        assert_eq!(
            TerminalText::sanitize(input.as_bytes()).as_str(),
            expected,
            "{input:?}"
        );
    }
    for (input, expected) in [("a\x1b]0;t\x07b\n", "ab\n"), ("a\x1bPq\x1b\\b", "ab")] {
        assert_eq!(
            TerminalText::sanitize(input.as_bytes()).as_str(),
            expected,
            "{input:?}"
        );
    }
    for bidi in BIDI {
        let input = format!("a{bidi}b");
        assert_eq!(TerminalText::sanitize(input.as_bytes()).as_str(), "ab");
    }
    assert_eq!(
        TerminalText::sanitize("access\u{202e} \u{2066}if admin\u{2069} \u{2066}".as_bytes())
            .as_str(),
        "access if admin "
    );
    for text in [
        "\u{5e9}\u{5dc}\u{5d5}\u{5dd}",
        "\u{645}\u{631}\u{62d}\u{628}\u{627}",
        "h\u{e9}llo",
    ] {
        assert_eq!(TerminalText::sanitize(text.as_bytes()).as_str(), text);
    }
}

#[test]
fn display_prints_the_sanitized_text() {
    let text = TerminalText::sanitize(b"a\x1b[31mb\nc");
    assert_eq!(text.to_string(), "ab\nc");
    assert_eq!(text.to_string(), text.as_str());
}
