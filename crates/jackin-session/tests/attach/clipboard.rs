// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::ClipboardGuard;

const WRITE_BEL: &[u8] = b"\x1b]52;c;SGVsbG8=\x07";
const WRITE_ST: &[u8] = b"\x1b]52;p;SGVsbG8=\x1b\\";
const QUERY_BEL: &[u8] = b"\x1b]52;c;?\x07";
const QUERY_ST: &[u8] = b"\x1b]52;c;?\x1b\\";
const TITLE: &[u8] = b"\x1b]0;title\x07";
const OSC10: &[u8] = b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\";
const OSC520: &[u8] = b"\x1b]520;x\x07";
const SGR: &[u8] = b"\x1b[1;31mred\x1b[0m";
const CPR: &[u8] = b"\x1b[12;40R";

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
            let expected: &[u8] = if at == 3 { b"ab\x1bcd" } else { b"abcd" };
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
fn other_sequences_and_plain_bytes_pass_unchanged_in_both_directions() {
    let samples = [
        TITLE,
        OSC10,
        OSC520,
        SGR,
        CPR,
        b"\x1b",
        b"\x1b]",
        b"\x1b]5",
        b"\x1b]5x",
        b"\x1b]52",
        b"\x1b]5;x\x07",
        b"plain \xe2\x9c\x93 text\r\n",
        b"\x1b\x1b]0;t\x07",
    ];
    for sample in samples {
        for at in 0..=sample.len() {
            let mut guard = ClipboardGuard::default();
            let mut out = guard.from_sandbox(&sample[..at]);
            out.extend(guard.from_sandbox(&sample[at..]));
            let mut tail = guard.from_sandbox(b"\x07z");
            out.append(&mut tail);
            assert_eq!(
                out,
                [sample, b"\x07z".as_slice()].concat(),
                "{sample:?} at {at}"
            );
            let mut guard = ClipboardGuard::default();
            let mut out = guard.from_operator(&sample[..at]);
            out.extend(guard.from_operator(&sample[at..]));
            let mut tail = guard.from_operator(b"\x07z");
            out.append(&mut tail);
            assert_eq!(
                out,
                [sample, b"\x07z".as_slice()].concat(),
                "{sample:?} at {at}"
            );
        }
    }
}

#[test]
fn an_escape_inside_an_osc52_body_ends_it_and_starts_the_next_sequence() {
    let mut guard = ClipboardGuard::default();
    let stream = framed(b"\x1b]52;c;QQ==\x1b[31mred");
    assert_eq!(guard.from_sandbox(&stream), b"ab\x1b[31mredcd");
}

#[test]
fn a_lone_escape_key_from_the_operator_is_not_held_back() {
    let mut guard = ClipboardGuard::default();
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
