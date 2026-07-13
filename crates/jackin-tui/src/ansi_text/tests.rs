// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `ansi_text`.
use super::*;

#[test]
fn strips_ansi_sequences_from_bytes() {
    assert_eq!(
        strip_bytes(b"\x1b[31merror\x1b[0m\n").as_slice(),
        b"error\n"
    );
}

#[test]
fn converts_sgr_to_styled_spans() {
    let spans = styled_spans(
        "plain \x1b[31mbad\x1b[0m ok",
        Style::default().fg(Color::Gray),
    );
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[1].content, "bad");
    assert_eq!(spans[1].style.fg, Some(Color::Red));
    assert_eq!(spans[2].style.fg, Some(Color::Gray));
}
