// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::ClipboardGuard;

const WRITE_BEL: &[u8] = b"\x1b]52;c;SGVsbG8=\x07";
const WRITE_ST: &[u8] = b"\x1b]52;p;SGVsbG8=\x1b\\";
const QUERY_BEL: &[u8] = b"\x1b]52;c;?\x07";
const QUERY_ST: &[u8] = b"\x1b]52;c;?\x1b\\";
const TITLE_BEL: &[u8] = b"\x1b]0;title\x07";
const TITLE_ST: &[u8] = b"\x1b]2;\xe2\xa0\x9c \xe2\x9c\x9d jackin\x1b\\";
const ICON: &[u8] = b"\x1b]1;icon\x07";
const HYPERLINK_OPEN: &[u8] = b"\x1b]8;id=a;https://example.com/x?y=1\x1b\\";
const HYPERLINK: &[u8] = b"\x1b]8;id=a;https://example.com/x?y=1\x1b\\link\x1b]8;;\x1b\\";
const COLOR_QUERY_10: &[u8] = b"\x1b]10;?\x1b\\";
const COLOR_QUERY_11: &[u8] = b"\x1b]11;?\x07";
const COLOR_REPLY_10: &[u8] = b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\";
const COLOR_REPLY_11: &[u8] = b"\x1b]11;rgb:1e1e/1e1e/1e1e\x07";
const SGR: &[u8] = b"\x1b[1;31mred\x1b[0m";
const CPR: &[u8] = b"\x1b[12;40R";
const DROPPED: [&[u8]; 14] = [
    b"\x1b]4;1;?\x07",
    b"\x1b]7;file:///tmp\x07",
    b"\x1b]9;notice\x07",
    b"\x1b]10;rgb:ffff/0000/0000\x07",
    b"\x1b]11;rgb:0000/0000/0000\x1b\\",
    b"\x1b]12;?\x07",
    b"\x1b]104\x07",
    b"\x1b]133;A\x07",
    b"\x1b]520;x\x07",
    b"\x1b]777;notify;a;b\x07",
    b"\x1b]1337;SetUserVar=a=Yg==\x07",
    b"\x1b]5522;type=read\x07",
    b"\x1b]0\x07",
    b"\x1b]\x07",
];

fn framed(middle: &[u8]) -> Vec<u8> {
    [b"ab".as_slice(), middle, b"cd".as_slice()].concat()
}

fn chunked(
    bytes: &[u8],
    at: usize,
    mut feed: impl FnMut(&mut ClipboardGuard, &[u8]) -> Vec<u8>,
) -> Vec<u8> {
    let mut guard = ClipboardGuard::default();
    let mut out = feed(&mut guard, &bytes[..at]);
    out.extend(feed(&mut guard, &bytes[at..]));
    out
}

#[test]
fn sandbox_output_loses_every_osc52_sequence_and_keeps_every_other_byte() {
    for osc52 in [WRITE_BEL, WRITE_ST, QUERY_BEL, QUERY_ST] {
        let mut guard = ClipboardGuard::default();
        assert_eq!(guard.from_sandbox(&framed(osc52)), b"abcd", "{osc52:?}");
    }
    let mut guard = ClipboardGuard::default();
    let two = [WRITE_BEL, b"mid".as_slice(), QUERY_ST].concat();
    assert_eq!(guard.from_sandbox(&framed(&two)), b"abmidcd");
}

#[test]
fn a_sequence_split_at_any_byte_of_the_stream_is_still_stripped() {
    let streams = [framed(WRITE_BEL), framed(WRITE_ST), framed(QUERY_ST)];
    for stream in streams {
        for at in 0..=stream.len() {
            let out = chunked(&stream, at, ClipboardGuard::from_sandbox);
            assert_eq!(out, b"abcd", "split at {at} of {stream:?}");
            let out = chunked(&stream, at, ClipboardGuard::from_operator);
            let expected: &[u8] = match at {
                3 => b"ab\x1bcd",
                4 => continue,
                _ => b"abcd",
            };
            assert_eq!(out, expected, "operator split at {at} of {stream:?}");
        }
    }
}

#[test]
fn operator_input_loses_every_osc52_reply() {
    for osc52 in [WRITE_BEL, WRITE_ST, QUERY_BEL, QUERY_ST] {
        let mut guard = ClipboardGuard::default();
        assert_eq!(guard.from_operator(&framed(osc52)), b"abcd", "{osc52:?}");
    }
}

#[test]
fn csi_sgr_and_plain_text_pass_unchanged_in_both_directions_at_every_cut() {
    let samples: [&[u8]; 8] = [
        SGR,
        CPR,
        b"plain \xe2\x9c\x9d \xe2\xa0\x9c \xc2\xb5 \xf0\x9f\x98\x80 text\r\n",
        b"\x1b[?2004h\x1b[>1u\x1b[<u\x1b[6n",
        b"\x1b(B\x1b=\x1b7\x1b8",
        b"\x1bP1$r0m\x1b\\",
        b"\x1b[200~pasted \x1b[0m text\x1b[201~",
        b"\x07\x08\t\r\n",
    ];
    for sample in samples {
        let expected = [sample, b"\x07z".as_slice()].concat();
        for at in 0..=sample.len() {
            for direction in [ClipboardGuard::from_sandbox, ClipboardGuard::from_operator] {
                let mut guard = ClipboardGuard::default();
                let mut out = direction(&mut guard, &sample[..at]);
                out.extend(direction(&mut guard, &sample[at..]));
                out.extend(direction(&mut guard, b"\x07z"));
                assert_eq!(out, expected, "{sample:?} at {at}");
            }
        }
    }
}

#[test]
fn sandbox_output_keeps_titles_hyperlinks_and_color_queries_and_drops_every_other_osc() {
    for allowed in [
        TITLE_BEL,
        TITLE_ST,
        ICON,
        HYPERLINK,
        COLOR_QUERY_10,
        COLOR_QUERY_11,
    ] {
        for at in 0..=allowed.len() {
            let stream = framed(allowed);
            let out = chunked(&stream, 2 + at, ClipboardGuard::from_sandbox);
            assert_eq!(out, stream, "{allowed:?} cut at {at}");
        }
    }
    for dropped in DROPPED {
        let mut guard = ClipboardGuard::default();
        assert_eq!(guard.from_sandbox(&framed(dropped)), b"abcd", "{dropped:?}");
    }
}

#[test]
fn operator_input_keeps_color_replies_and_drops_every_other_osc() {
    for allowed in [COLOR_REPLY_10, COLOR_REPLY_11] {
        let mut guard = ClipboardGuard::default();
        assert_eq!(guard.from_operator(&framed(allowed)), framed(allowed));
    }
    let colors = [b"\x1b]10;".as_slice(), b"\x1b]11;".as_slice()];
    let others = DROPPED
        .into_iter()
        .filter(|sequence| !colors.iter().any(|prefix| sequence.starts_with(prefix)));
    for dropped in others.chain([TITLE_BEL, TITLE_ST, ICON, HYPERLINK_OPEN, COLOR_QUERY_10]) {
        let mut guard = ClipboardGuard::default();
        assert_eq!(
            guard.from_operator(&framed(dropped)),
            b"abcd",
            "{dropped:?}"
        );
    }
}

#[test]
fn an_allowed_sequence_with_a_hidden_control_or_c1_byte_is_dropped_whole() {
    let hostile: [&[u8]; 6] = [
        b"\x1b]0;a\x00b\x07",
        b"\x1b]0;a\x7fb\x07",
        b"\x1b]0;a\xc2\x85b\x07",
        b"\x1b]0;a\xffb\x07",
        b"\x1b]00;a\x07",
        b"\x1b]10;?;x\x07",
    ];
    for sequence in hostile {
        let mut guard = ClipboardGuard::default();
        assert_eq!(
            guard.from_sandbox(&framed(sequence)),
            b"abcd",
            "{sequence:?}"
        );
    }
}

#[test]
fn an_oversized_osc_is_dropped_without_growing_memory_and_later_output_survives() {
    let mut guard = ClipboardGuard::default();
    let mut stream = b"ab\x1b]0;".to_vec();
    stream.extend(std::iter::repeat_n(b'x', 1_000_000));
    stream.extend_from_slice(b"\x07cd");
    let out = guard.from_sandbox(&stream);
    assert_eq!(out.len(), 4, "{} bytes came through", out.len());
    assert_eq!(out, b"abcd");
}

#[test]
fn an_escape_inside_an_osc_body_ends_it_and_starts_the_next_sequence() {
    let mut guard = ClipboardGuard::default();
    let stream = framed(b"\x1b]52;c;QQ==\x1b[31mred");
    assert_eq!(guard.from_sandbox(&stream), b"ab\x1b[31mredcd");
}

#[test]
fn a_cancelled_osc_ends_at_can_or_sub_and_later_output_stays_visible() {
    for cancel in [0x18_u8, 0x1a] {
        for body in [&b"\x1b]52;c;QUJD"[..], b"\x1b]0;title"] {
            let mut guard = ClipboardGuard::default();
            let first = guard.from_sandbox(&[b"ab", body, &[cancel]].concat());
            let second = guard.from_sandbox(b"visible \x1b[1mtext\r\n");
            let third = guard.from_sandbox(b"more\x07tail");
            assert_eq!(first, b"ab", "{body:?} {cancel}");
            assert_eq!(second, b"visible \x1b[1mtext\r\n", "{body:?} {cancel}");
            assert_eq!(third, b"more\x07tail", "{body:?} {cancel}");
        }
    }
}

#[test]
fn alt_right_bracket_and_the_escape_key_from_the_operator_are_not_held_back() {
    let mut guard = ClipboardGuard::default();
    assert_eq!(guard.from_operator(b"\x1b]"), b"\x1b]");
    assert_eq!(guard.from_operator(b"x"), b"x");
    assert_eq!(guard.from_operator(b"\x1b"), b"\x1b");
    assert_eq!(guard.from_operator(b"x"), b"x");
}

#[test]
fn directions_keep_separate_state() {
    let mut guard = ClipboardGuard::default();
    assert_eq!(guard.from_sandbox(b"a\x1b]52;c;"), b"a");
    assert_eq!(guard.from_operator(b"typed"), b"typed");
    assert_eq!(guard.from_sandbox(b"QQ==\x07b"), b"b");
}
