// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `host_claude`.
use super::*;

#[test]
fn parse_version_line_takes_first_whitespace_token() {
    assert_eq!(
        parse_version_line("2.1.4 (Claude Code)\n"),
        Some("2.1.4".to_owned())
    );
}

#[test]
fn parse_version_line_trims_leading_whitespace() {
    assert_eq!(
        parse_version_line("  3.0.0-beta.1\n"),
        Some("3.0.0-beta.1".to_owned())
    );
}

#[test]
fn parse_version_line_returns_none_for_empty() {
    assert_eq!(parse_version_line(""), None);
    assert_eq!(parse_version_line("   \n  "), None);
}

#[test]
fn forward_redacted_line_captures_token_and_redacts_output() {
    let mut captured = None;
    let mut out = Vec::new();
    forward_redacted_line(
        b"sk-ant-oat01-EXAMPLE save this securely\n",
        &mut captured,
        &mut out,
    );
    assert_eq!(captured.as_deref(), Some("sk-ant-oat01-EXAMPLE"));
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s, "<redacted> save this securely\n");
}

#[test]
fn forward_redacted_line_passes_non_token_lines_verbatim() {
    let mut captured = None;
    let mut out = Vec::new();
    forward_redacted_line(b"Open this URL in your browser:\n", &mut captured, &mut out);
    assert!(captured.is_none());
    assert_eq!(out, b"Open this URL in your browser:\n");
}

/// Regression: PTY chunks may contain invalid UTF-8 (terminal
/// escape garbage, mid-codepoint splits). The redactor must
/// scan and slice in raw bytes — going through
/// `String::from_utf8_lossy` would substitute every invalid
/// byte with U+FFFD (3 bytes) and shift offsets so the slice
/// back into the original `&[u8]` would be wrong (or panic).
#[test]
fn forward_redacted_line_handles_invalid_utf8_before_token() {
    let mut captured = None;
    let mut out = Vec::new();
    // 0xFF / 0xFE are invalid UTF-8 lead bytes.
    let mut line = vec![0xFFu8, 0xFE];
    line.extend_from_slice(b" sk-ant-oat01-EXAMPLE done\n");
    forward_redacted_line(&line, &mut captured, &mut out);
    assert_eq!(captured.as_deref(), Some("sk-ant-oat01-EXAMPLE"));
    // Surrounding bytes (including the invalid pair) are
    // preserved verbatim; only the token is redacted.
    let mut expected = vec![0xFFu8, 0xFE];
    expected.extend_from_slice(b" <redacted> done\n");
    assert_eq!(out, expected);
}

#[test]
fn forward_redacted_line_only_captures_first_token() {
    let mut captured = Some("sk-ant-oat01-FIRST".to_owned());
    let mut out = Vec::new();
    forward_redacted_line(b"sk-ant-oat01-SECOND\n", &mut captured, &mut out);
    // Already captured: do not overwrite.
    assert_eq!(captured.as_deref(), Some("sk-ant-oat01-FIRST"));
    // Still redact the second occurrence so it never echoes.
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s, "<redacted>\n");
}

/// Regression: claude CLI splits the token display across two visual
/// rows using cursor-down escapes (`\x1b[1B`) and color codes. The
/// extractor must skip these and reassemble the full token.
#[test]
fn forward_redacted_line_captures_token_split_by_ansi_escapes() {
    let mut captured = None;
    let mut out = Vec::new();
    // Pattern observed in production: color, first chunk, cursor-down,
    // color-reset, erase-line, cursor-down, space, color, second chunk, reset.
    let line: &[u8] = b"\x1b[38;2;255;193;7msk-ant-oat01-AAAA\x1b[1B\x1b[39m\x1b[K\x1b[1B \x1b[38;2;255;193;7mBBBB\x1b[0m\n";
    forward_redacted_line(line, &mut captured, &mut out);
    assert_eq!(
        captured.as_deref(),
        Some("sk-ant-oat01-AAAABBBB"),
        "token must include both chunks"
    );
    let s = String::from_utf8_lossy(&out);
    assert!(!s.contains("AAAA"), "first chunk must be redacted");
    assert!(!s.contains("BBBB"), "second chunk must be redacted");
    assert!(s.contains("<redacted>"), "redacted marker must appear");
}

#[test]
fn drain_pty_buffer_processes_complete_lines_only() {
    let mut buf = b"banner\nsk-ant-oat01-X\nincomplete".to_vec();
    let mut captured = None;
    let mut out = Vec::new();
    drain_pty_buffer(&mut buf, &mut captured, &mut out);
    assert_eq!(captured.as_deref(), Some("sk-ant-oat01-X"));
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s, "banner\n<redacted>\n");
    // The incomplete tail stays in the buffer.
    assert_eq!(buf, b"incomplete");
}
