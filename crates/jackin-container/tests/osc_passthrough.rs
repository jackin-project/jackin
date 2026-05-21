/// OSC and unhandled-CSI passthrough contracts.
///
/// The roadmap requires every OSC sequence the agent emits to reach
/// the attached client when (and only when) the session owns the
/// focused pane. The `vt100` parser silently consumes OSC by default;
/// the `OscCapture` callback layer is what re-emits them. These tests
/// pin the contract by feeding raw OSC byte sequences into the parser
/// and asserting that `drain_passthrough` yields the same bytes back.
use jackin_container::session::OscCapture;
use vt100::Parser;

fn drained(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut p = Parser::new_with_callbacks(24, 80, 0, OscCapture::default());
    p.process(bytes);
    p.callbacks_mut().drain()
}

#[test]
fn osc_52_clipboard_write_is_re_emitted() {
    // Claude Code's "copy" button emits OSC 52 with base64 payload.
    // Regression: without the capture layer the write is silently
    // dropped at the multiplexer boundary.
    let drained = drained(b"\x1b]52;c;SGVsbG8=\x07");
    assert_eq!(drained.len(), 1);
    let s = &drained[0];
    assert!(s.starts_with(b"\x1b]52;"));
    assert!(s.ends_with(b"\x07"));
    assert!(s.windows(8).any(|w| w == b"SGVsbG8="));
}

#[test]
fn osc_2_window_title_is_re_emitted_and_captured() {
    let mut p = Parser::new_with_callbacks(24, 80, 0, OscCapture::default());
    p.process(b"\x1b]2;Claude (working)\x07");
    assert_eq!(
        p.callbacks().title.as_deref(),
        Some("Claude (working)"),
        "title not captured"
    );
    let drained = p.callbacks_mut().drain();
    assert_eq!(drained.len(), 1);
    assert!(drained[0].starts_with(b"\x1b]2;"));
}

#[test]
fn osc_8_hyperlink_is_re_emitted() {
    // OSC 8 hyperlinks: `\x1b]8;;<url>\x1b\\<text>\x1b]8;;\x1b\\`.
    // The two OSC frames must round-trip.
    let drained = drained(b"\x1b]8;;https://example/\x07text\x1b]8;;\x07");
    assert!(
        drained.len() >= 2,
        "expected two OSC frames, got {drained:?}"
    );
    assert!(drained.iter().any(|f| f.starts_with(b"\x1b]8;;https")));
}

#[test]
fn osc_9_notification_is_re_emitted() {
    let drained = drained(b"\x1b]9;build finished\x07");
    assert_eq!(drained.len(), 1);
    let s = String::from_utf8_lossy(&drained[0]);
    assert!(s.contains("9;build finished"));
}

#[test]
fn unhandled_csi_kitty_keyboard_push_is_re_emitted() {
    // `\x1b[>1u` — push kitty keyboard protocol flags. vt100 doesn't
    // track this; without re-emission the outer terminal stays in
    // legacy encoding and the agent gets degraded key encoding back.
    let drained = drained(b"\x1b[>1u");
    assert!(
        drained.iter().any(|f| f == b"\x1b[>1u"),
        "drained: {drained:?}"
    );
}

#[test]
fn unhandled_csi_modify_other_keys_is_re_emitted() {
    let drained = drained(b"\x1b[>4;2m");
    assert!(
        drained.iter().any(|f| f == b"\x1b[>4;2m"),
        "drained: {drained:?}"
    );
}

#[test]
fn unhandled_csi_bsu_esu_is_re_emitted() {
    // Synchronised output markers — agents wrap each frame so the
    // outer terminal paints atomically.
    let drained = drained(b"\x1b[?2026h");
    assert!(
        drained.iter().any(|f| f == b"\x1b[?2026h"),
        "drained: {drained:?}"
    );
}

#[test]
fn known_csi_does_not_double_emit() {
    // Cursor positioning `\x1b[5;3H` is handled by vt100 itself; the
    // unhandled-CSI callback must not re-emit it (which would create
    // a duplicate cursor move on the client).
    let drained = drained(b"\x1b[5;3H");
    assert!(
        drained.iter().all(|f| !f.ends_with(b"H")),
        "vt100-handled CSI leaked through: {drained:?}"
    );
}

#[test]
fn drain_returns_empty_when_no_passthrough_emitted() {
    let drained = drained(b"plain text without any escape sequences");
    assert!(drained.is_empty());
}

#[test]
fn drain_clears_pending_between_calls() {
    let mut p = Parser::new_with_callbacks(24, 80, 0, OscCapture::default());
    p.process(b"\x1b]52;c;AAAA\x07");
    let first = p.callbacks_mut().drain();
    assert_eq!(first.len(), 1);
    let second = p.callbacks_mut().drain();
    assert!(
        second.is_empty(),
        "drain must clear pending; got {second:?}"
    );
}
