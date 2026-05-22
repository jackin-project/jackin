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
fn osc_7_cwd_is_captured_and_percent_decoded() {
    // Shell with starship: `\x1b]7;file://host/Users/alice/My%20Code\x07`
    let mut p = Parser::new_with_callbacks(24, 80, 0, OscCapture::default());
    p.process(b"\x1b]7;file://localhost/Users/alice/My%20Code\x07");
    assert_eq!(
        p.callbacks().cwd(),
        Some("/Users/alice/My Code"),
        "OSC 7 must percent-decode and strip the host"
    );
}

#[test]
fn osc_7_rejects_malformed_payload() {
    // Bare text without a `file://` scheme must not silently
    // overwrite the captured cwd — that surface is reserved for
    // valid URLs only.
    let mut p = Parser::new_with_callbacks(24, 80, 0, OscCapture::default());
    p.process(b"\x1b]7;random-text\x07");
    assert!(p.callbacks().cwd().is_none());
}

#[test]
fn kitty_kb_stack_tracks_push_and_pop() {
    let mut p = Parser::new_with_callbacks(24, 80, 0, OscCapture::default());
    p.process(b"\x1b[>1u\x1b[>3u");
    assert_eq!(p.callbacks().kitty_kb_stack(), &[1u16, 3]);
    // vte's CSI state machine treats `<` as a private marker only
    // when an explicit numeric param follows. Use the spec's full
    // form `\x1b[<{n}u` for portable pop, matching what kitty's own
    // docs prescribe.
    p.process(b"\x1b[<1u");
    assert_eq!(p.callbacks().kitty_kb_stack(), &[1u16]);
    p.process(b"\x1b[<5u"); // over-pop bounded by stack length
    assert!(p.callbacks().kitty_kb_stack().is_empty());
}

#[test]
fn kitty_kb_stack_caps_pathological_push() {
    // A buggy or hostile agent loops `\x1b[>1u`. The stack must
    // not grow without bound; cap is documented as 64.
    let mut p = Parser::new_with_callbacks(24, 80, 0, OscCapture::default());
    for _ in 0..200 {
        p.process(b"\x1b[>1u");
    }
    assert!(p.callbacks().kitty_kb_stack().len() <= 64);
}

#[test]
fn focus_events_flag_tracks_dec_1004() {
    let mut p = Parser::new_with_callbacks(24, 80, 0, OscCapture::default());
    p.process(b"\x1b[?1004h");
    assert!(p.callbacks().focus_events());
    p.process(b"\x1b[?1004l");
    assert!(!p.callbacks().focus_events());
}

#[test]
fn unhandled_csi_kitty_keyboard_push_is_suppressed() {
    // `\x1b[>1u` — push kitty keyboard protocol flags. NOT forwarded
    // to the outer terminal because doing so leaves every other pane
    // (shells, pre-mount agents) receiving operator keystrokes in
    // kitty-CSI-u form and surfacing them as garbage text. The
    // agent's own vt100 parses kitty keys inside its own screen
    // state; the outer terminal stays in plain CSI mode.
    let drained = drained(b"\x1b[>1u");
    assert!(
        drained.iter().all(|f| f != b"\x1b[>1u"),
        "kitty push must not reach the outer terminal: {drained:?}"
    );
}

#[test]
fn unhandled_csi_kitty_keyboard_pop_is_suppressed() {
    let drained = drained(b"\x1b[<u");
    assert!(
        drained.iter().all(|f| f != b"\x1b[<u"),
        "kitty pop must not reach the outer terminal: {drained:?}"
    );
}

#[test]
fn unhandled_csi_xterm_window_reports_are_suppressed() {
    // `CSI ... t` is xterm's window manipulation / reporting family.
    // Forwarding requests like `CSI 18t` to Ghostty makes Ghostty
    // answer on the client input stream; under resize those replies
    // can land in a shell pane as visible `8;rows;cols t` garbage.
    let drained = drained(b"\x1b[18t\x1b[14t\x1b[16t\x1b[8;40;135t");
    assert!(
        drained.iter().all(|f| !f.ends_with(b"t")),
        "xterm window reports must not reach the outer terminal: {drained:?}"
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
fn unhandled_csi_bsu_esu_is_forwarded() {
    // Synchronised output (`?2026`) is not tracked locally — vt100
    // does not handle it, and we have no render-defer logic in the
    // daemon yet. Forward verbatim so an outer terminal that
    // understands BSU/ESU still gets to apply atomic frames.
    let drained = drained(b"\x1b[?2026h");
    assert!(
        drained.iter().any(|f| f == b"\x1b[?2026h"),
        "?2026h must reach the outer terminal: {drained:?}"
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
