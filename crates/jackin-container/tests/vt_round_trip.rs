/// VT round-trip contracts the multiplexer must preserve.
///
/// Each test feeds bytes through a `vt100::Parser` and asserts that
/// the resulting `Screen` either reflects the agent's intent (mouse
/// mode, bracketed paste, alt-screen) or that the rendered output
/// reproduces it. These are the regressions the hand-rolled `vte`
/// emulator could not satisfy.
use jackin_container::render::render_pane;
use vt100::{MouseProtocolMode, Parser};

#[test]
fn alt_screen_round_trip_preserves_primary_content() {
    let mut p = Parser::new(5, 20, 0);
    p.process(b"primary\r\nview\r\n");
    let before = p.screen().contents();
    p.process(b"\x1b[?1049h\x1b[2J\x1b[Halt content\r\n");
    p.process(b"\x1b[?1049l");
    let after = p.screen().contents();
    assert_eq!(after.trim_end(), before.trim_end());
}

#[test]
fn bracketed_paste_mode_tracked_by_screen() {
    let mut p = Parser::new(5, 20, 0);
    assert!(!p.screen().bracketed_paste());
    p.process(b"\x1b[?2004h");
    assert!(p.screen().bracketed_paste());
    p.process(b"\x1b[?2004l");
    assert!(!p.screen().bracketed_paste());
}

#[test]
fn mouse_modes_tracked_by_screen() {
    let mut p = Parser::new(5, 20, 0);
    assert!(matches!(
        p.screen().mouse_protocol_mode(),
        MouseProtocolMode::None
    ));
    p.process(b"\x1b[?1003h");
    assert!(matches!(
        p.screen().mouse_protocol_mode(),
        MouseProtocolMode::ButtonMotion | MouseProtocolMode::AnyMotion
    ));
}

#[test]
fn application_cursor_mode_tracked_by_screen() {
    let mut p = Parser::new(5, 20, 0);
    assert!(!p.screen().application_cursor());
    p.process(b"\x1b[?1h");
    assert!(p.screen().application_cursor());
}

#[test]
fn hide_cursor_tracked_by_screen() {
    let mut p = Parser::new(5, 20, 0);
    assert!(!p.screen().hide_cursor());
    p.process(b"\x1b[?25l");
    assert!(p.screen().hide_cursor());
    p.process(b"\x1b[?25h");
    assert!(!p.screen().hide_cursor());
}

#[test]
fn render_pane_includes_content_at_offset() {
    let mut p = Parser::new(3, 10, 0);
    p.process(b"hello");
    let mut buf = Vec::new();
    render_pane(p.screen(), 2, 4, 3, 10, false, &mut buf);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("hello"));
    assert!(s.contains("\x1b[3;5H")); // dest_row=2, dest_col=4 → 1-based 3,5
}
