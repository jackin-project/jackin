// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{DEFAULT_SPOOL_CAPACITY_BYTES, SessionSpool, sanitize_terminal_text};

#[test]
fn sanitizer_strips_csi_osc_and_controls() {
    let raw = "\u{1b}[1;32mok\u{1b}[0m\u{1b}]0;title\u{7}\u{1b}M\nline\x00two\x07";
    assert_eq!(sanitize_terminal_text(raw), "ok\nlinetwo");
    assert!(!sanitize_terminal_text(raw).contains('\u{1b}'));
}

#[test]
fn spool_evicts_oldest_and_flags_gap() {
    let mut spool = SessionSpool::new(8);
    spool.push("0123456789ABCDEF");
    assert!(spool.has_gap());
    assert_eq!(spool.retained_bytes(), 8);
    assert_eq!(spool.read().text, "89ABCDEF");
    assert!(spool.read().gap);
}

#[test]
fn spool_without_overflow_has_no_gap() {
    let mut spool = SessionSpool::new(DEFAULT_SPOOL_CAPACITY_BYTES);
    spool.push("hello\n");
    assert!(!spool.has_gap());
    assert_eq!(spool.read().text, "hello\n");
}

#[test]
fn sealed_spool_ignores_pushes() {
    let mut spool = SessionSpool::new(64);
    spool.push("kept");
    spool.seal();
    spool.push("dropped");
    assert_eq!(spool.read().text, "kept");
    assert!(!spool.has_gap());
}
