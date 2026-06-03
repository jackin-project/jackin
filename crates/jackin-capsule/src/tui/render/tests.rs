//! Tests for `render`.
use super::*;
use vt100::Parser;

#[test]
fn alt_screen_round_trip_preserves_primary() {
    // Enter alt-screen, write content, leave alt-screen, primary should
    // be restored. Regression guard for the hand-rolled emulator that
    // ignored DEC private mode `?1049`.
    let mut parser = Parser::new(5, 20, 0);
    parser.process(b"hello\r\nworld\r\n");
    let primary_before = parser.screen().contents();

    parser.process(b"\x1b[?1049h");
    parser.process(b"\x1b[2J\x1b[Halt-screen content\r\n");
    parser.process(b"\x1b[?1049l");

    let primary_after = parser.screen().contents();
    assert_eq!(
        primary_after.trim_end(),
        primary_before.trim_end(),
        "primary screen lost across alt-screen entry/exit"
    );
}

#[test]
fn render_pane_offsets_cursor_to_origin() {
    let mut parser = Parser::new(3, 10, 0);
    parser.process(b"hi");
    let mut buf = Vec::new();
    render_pane(parser.screen(), 4, 2, 3, 10, PaneBodyDim::Normal, &mut buf);
    let s = String::from_utf8_lossy(&buf);
    // Render must start by writing to row 5 col 3 (1-based after the
    // dest_row=4, dest_col=2 offset) — not row 1 col 1 which would
    // mean the offset was dropped.
    assert!(
        s.contains("\x1b[5;3H"),
        "missing pane-origin cursor move: {s:?}"
    );
}

#[test]
fn inactive_pane_dim_uses_light_ansi_dim_only() {
    let mut parser = Parser::new(1, 10, 0);
    parser.process(b"\x1b[31mred");
    let mut buf = Vec::new();
    render_pane(
        parser.screen(),
        0,
        0,
        1,
        10,
        PaneBodyDim::Inactive,
        &mut buf,
    );
    let out = String::from_utf8_lossy(&buf);

    assert!(
        out.contains("\x1b[0;2;31mred"),
        "inactive pane should keep normal color codes with ANSI dim: {out:?}"
    );
}

#[test]
fn pane_cache_first_render_is_full_and_tracks_every_visible_row() {
    let mut parser = Parser::new(3, 8, 0);
    parser.process(b"one\r\ntwo");
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();

    let stats = cache.render_partial(parser.screen(), 10, 20, 3, 8, PaneBodyDim::Normal, &mut buf);

    assert_eq!(stats.mode, PaneBodyRenderMode::Full);
    assert_eq!(stats.changed_rows, vec![0, 1, 2]);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("\x1b[11;21H"));
    assert!(s.contains("\x1b[12;21H"));
    assert!(s.contains("\x1b[13;21H"));
}

#[test]
fn pane_cache_emits_only_changed_rows_after_warmup() {
    let mut parser = Parser::new(3, 12, 0);
    parser.process(b"alpha\r\nbeta\r\ngamma");
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();
    cache.render_full(parser.screen(), 0, 0, 3, 12, PaneBodyDim::Normal, &mut buf);
    buf.clear();

    parser.process(b"\x1b[2;1Hbravo");
    let stats = cache.render_partial(parser.screen(), 0, 0, 3, 12, PaneBodyDim::Normal, &mut buf);

    assert_eq!(stats.mode, PaneBodyRenderMode::Partial);
    assert_eq!(stats.changed_rows, vec![1]);
    let s = String::from_utf8_lossy(&buf);
    assert!(!s.contains("\x1b[1;1H"));
    assert!(s.contains("\x1b[2;1H"));
    assert!(!s.contains("\x1b[3;1H"));
    assert!(s.contains("bravo"));
}

#[test]
fn pane_cache_partial_rows_reset_styles_independently() {
    let mut parser = Parser::new(2, 16, 0);
    parser.process(b"\x1b[31mred\x1b[0m\r\nplain");
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();
    cache.render_full(parser.screen(), 0, 0, 2, 16, PaneBodyDim::Normal, &mut buf);
    buf.clear();

    parser.process(b"\x1b[1;1H\x1b[32mgreen\x1b[0m");
    let stats = cache.render_partial(parser.screen(), 0, 0, 2, 16, PaneBodyDim::Normal, &mut buf);

    assert_eq!(stats.changed_rows, vec![0]);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("\x1b[1;1H\x1b[0m"));
    assert!(s.contains("\x1b[0;32mgreen"));
    assert!(s.ends_with("\x1b[0m"));
}

#[test]
fn pane_cache_handles_wide_characters_without_dirtying_continuations() {
    let mut parser = Parser::new(2, 10, 0);
    parser.process("表x\r\nsame".as_bytes());
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();
    cache.render_full(parser.screen(), 0, 0, 2, 10, PaneBodyDim::Normal, &mut buf);
    buf.clear();

    parser.process("\x1b[1;3Hy".as_bytes());
    let stats = cache.render_partial(parser.screen(), 0, 0, 2, 10, PaneBodyDim::Normal, &mut buf);

    assert_eq!(stats.changed_rows, vec![0]);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("表y"));
    assert!(!s.contains("表 y"));
}

#[test]
fn pane_cache_partial_ansi_serialization_covers_rgb_and_background() {
    let mut parser = Parser::new(1, 8, 0);
    parser.process(b"plain");
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();
    cache.render_full(parser.screen(), 0, 0, 1, 8, PaneBodyDim::Normal, &mut buf);
    buf.clear();

    parser.process(b"\x1b[1;1H\x1b[38;2;1;2;3;48;5;4;1mX");
    let stats = cache.render_partial(parser.screen(), 0, 0, 1, 8, PaneBodyDim::Normal, &mut buf);

    assert_eq!(stats.changed_rows, vec![0]);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("\x1b[0;1;38;2;1;2;3;44mX"));
}
