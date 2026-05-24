/// VT round-trip contracts the multiplexer must preserve.
///
/// Each test feeds bytes through a `vt100::Parser` and asserts that
/// the resulting `Screen` either reflects the agent's intent (mouse
/// mode, bracketed paste, alt-screen) or that the rendered output
/// reproduces it. These are the regressions the hand-rolled `vte`
/// emulator could not satisfy.
use jackin_capsule::render::{PaneBodyCache, PaneBodyDim, PaneBodyRenderMode, render_pane};
use vt100::{MouseProtocolEncoding, MouseProtocolMode, Parser};

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
    p.process(b"\x1b[?1006h");
    assert!(matches!(
        p.screen().mouse_protocol_encoding(),
        MouseProtocolEncoding::Sgr
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
    render_pane(p.screen(), 2, 4, 3, 10, PaneBodyDim::Normal, &mut buf);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("hello"));
    assert!(s.contains("\x1b[3;5H")); // dest_row=2, dest_col=4 → 1-based 3,5
}

#[test]
fn render_pane_skips_wide_continuation_cells() {
    let mut p = Parser::new(2, 10, 0);
    p.process("表x".as_bytes());
    let mut buf = Vec::new();
    render_pane(p.screen(), 0, 0, 2, 10, PaneBodyDim::Normal, &mut buf);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("表x"));
    assert!(!s.contains("表 x"));
}

#[test]
fn small_vt_update_emits_partial_pane_body_redraw() {
    let mut p = Parser::new(4, 16, 0);
    p.process(b"row-one\r\nrow-two\r\nrow-three");
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();
    cache.render_full(p.screen(), 2, 3, 4, 16, PaneBodyDim::Normal, &mut buf);
    buf.clear();

    p.process(b"\x1b[2;1HROW-TWO");
    let stats = cache.render_partial(p.screen(), 2, 3, 4, 16, PaneBodyDim::Normal, &mut buf);

    assert_eq!(stats.mode, PaneBodyRenderMode::Partial);
    assert_eq!(stats.changed_rows, vec![1]);
    let rendered = String::from_utf8_lossy(&buf);
    assert!(!rendered.contains("\x1b[3;4H")); // pane row 0
    assert!(rendered.contains("\x1b[4;4H")); // pane row 1
    assert!(!rendered.contains("\x1b[5;4H")); // pane row 2
    assert!(rendered.contains("ROW-TWO"));
}

#[test]
fn vt100_clears_scrollback_without_resetting_modes() {
    let mut p = Parser::new(2, 8, 10);
    p.process(b"\x1b[?2004hone\r\ntwo\r\nthree");
    p.screen_mut().set_scrollback(usize::MAX);
    assert!(p.screen().scrollback() > 0);

    p.screen_mut().clear_scrollback();

    assert_eq!(p.screen().scrollback(), 0);
    assert!(p.screen().bracketed_paste());
}

#[test]
fn vt100_csi_3j_clears_scrollback() {
    let mut p = Parser::new(2, 8, 10);
    p.process(b"one\r\ntwo\r\nthree");
    p.screen_mut().set_scrollback(usize::MAX);
    assert!(p.screen().scrollback() > 0);

    p.process(b"\x1b[3J");

    assert_eq!(p.screen().scrollback(), 0);
}

#[test]
fn vt100_keeps_scrollback_view_offset_during_output() {
    let mut p = Parser::new(2, 8, 10);
    p.process(b"one\r\ntwo\r\nthree");
    p.screen_mut().set_scrollback(1);

    p.process(b"!");

    assert_eq!(p.screen().scrollback(), 1);
}
