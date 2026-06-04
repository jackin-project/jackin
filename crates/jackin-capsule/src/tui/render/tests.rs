//! Tests for `render`.
use super::*;
use jackin_term::DamageGrid;

/// Build a viewport snapshot from a freshly-fed `DamageGrid` the same way
/// the production compositor does, so these tests exercise the real
/// snapshot-render path.
fn snapshot(grid: &DamageGrid, rows: u16, cols: u16) -> Vec<RowSnapshot> {
    pane_snapshot_from_damagegrid(grid, rows, cols)
}

fn grid_with(rows: u16, cols: u16, bytes: &[u8]) -> DamageGrid {
    let mut grid = DamageGrid::new(rows, cols, 0);
    grid.process(bytes);
    grid
}

#[test]
fn alt_screen_round_trip_preserves_primary() {
    // Enter alt-screen, write content, leave alt-screen, primary should
    // be restored. Regression guard for ignoring DEC private mode `?1049`.
    let mut grid = DamageGrid::new(5, 20, 0);
    grid.process(b"hello\r\nworld\r\n");
    let primary_before = grid.dump().to_text();

    grid.process(b"\x1b[?1049h");
    grid.process(b"\x1b[2J\x1b[Halt-screen content\r\n");
    grid.process(b"\x1b[?1049l");

    let primary_after = grid.dump().to_text();
    assert_eq!(
        primary_after.trim_end(),
        primary_before.trim_end(),
        "primary screen lost across alt-screen entry/exit"
    );
}

#[test]
fn render_pane_offsets_cursor_to_origin() {
    let grid = grid_with(3, 10, b"hi");
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();
    cache.render_full_snapshot(
        snapshot(&grid, 3, 10),
        4,
        2,
        3,
        10,
        PaneBodyDim::Normal,
        PaneRightEdge::TerminalEdge,
        &mut buf,
    );
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
    let grid = grid_with(1, 10, b"\x1b[31mred");
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();
    cache.render_full_snapshot(
        snapshot(&grid, 1, 10),
        0,
        0,
        1,
        10,
        PaneBodyDim::Inactive,
        PaneRightEdge::TerminalEdge,
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
    let grid = grid_with(3, 8, b"one\r\ntwo");
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();

    let stats = cache.render_partial_snapshot(
        snapshot(&grid, 3, 8),
        10,
        20,
        3,
        8,
        PaneBodyDim::Normal,
        PaneRightEdge::TerminalEdge,
        &mut buf,
    );

    assert_eq!(stats.mode, PaneBodyRenderMode::Full);
    assert_eq!(stats.changed_rows, vec![0, 1, 2]);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("\x1b[11;21H"));
    assert!(s.contains("\x1b[12;21H"));
    assert!(s.contains("\x1b[13;21H"));
}

#[test]
fn pane_cache_emits_only_changed_rows_after_warmup() {
    let mut grid = grid_with(3, 12, b"alpha\r\nbeta\r\ngamma");
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();
    cache.render_full_snapshot(
        snapshot(&grid, 3, 12),
        0,
        0,
        3,
        12,
        PaneBodyDim::Normal,
        PaneRightEdge::TerminalEdge,
        &mut buf,
    );
    buf.clear();

    grid.process(b"\x1b[2;1Hbravo");
    let stats = cache.render_partial_snapshot(
        snapshot(&grid, 3, 12),
        0,
        0,
        3,
        12,
        PaneBodyDim::Normal,
        PaneRightEdge::TerminalEdge,
        &mut buf,
    );

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
    let mut grid = grid_with(2, 16, b"\x1b[31mred\x1b[0m\r\nplain");
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();
    cache.render_full_snapshot(
        snapshot(&grid, 2, 16),
        0,
        0,
        2,
        16,
        PaneBodyDim::Normal,
        PaneRightEdge::TerminalEdge,
        &mut buf,
    );
    buf.clear();

    grid.process(b"\x1b[1;1H\x1b[32mgreen\x1b[0m");
    let stats = cache.render_partial_snapshot(
        snapshot(&grid, 2, 16),
        0,
        0,
        2,
        16,
        PaneBodyDim::Normal,
        PaneRightEdge::TerminalEdge,
        &mut buf,
    );

    assert_eq!(stats.changed_rows, vec![0]);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("\x1b[1;1H\x1b[0m"));
    assert!(s.contains("\x1b[0;32mgreen"));
    assert!(s.ends_with("\x1b[0m"));
}

#[test]
fn pane_cache_handles_wide_characters_without_dirtying_continuations() {
    let mut grid = grid_with(2, 10, "表x\r\nsame".as_bytes());
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();
    cache.render_full_snapshot(
        snapshot(&grid, 2, 10),
        0,
        0,
        2,
        10,
        PaneBodyDim::Normal,
        PaneRightEdge::TerminalEdge,
        &mut buf,
    );
    buf.clear();

    grid.process("\x1b[1;3Hy".as_bytes());
    let stats = cache.render_partial_snapshot(
        snapshot(&grid, 2, 10),
        0,
        0,
        2,
        10,
        PaneBodyDim::Normal,
        PaneRightEdge::TerminalEdge,
        &mut buf,
    );

    assert_eq!(stats.changed_rows, vec![0]);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("表y"));
    assert!(!s.contains("表 y"));
}

#[test]
fn pane_cache_partial_ansi_serialization_covers_rgb_and_background() {
    let mut grid = grid_with(1, 8, b"plain");
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();
    cache.render_full_snapshot(
        snapshot(&grid, 1, 8),
        0,
        0,
        1,
        8,
        PaneBodyDim::Normal,
        PaneRightEdge::TerminalEdge,
        &mut buf,
    );
    buf.clear();

    grid.process(b"\x1b[1;1H\x1b[38;2;1;2;3;48;5;4;1mX");
    let stats = cache.render_partial_snapshot(
        snapshot(&grid, 1, 8),
        0,
        0,
        1,
        8,
        PaneBodyDim::Normal,
        PaneRightEdge::TerminalEdge,
        &mut buf,
    );

    assert_eq!(stats.changed_rows, vec![0]);
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("\x1b[0;1;38;2;1;2;3;44mX"));
}

// Defect 44 regression: after a resize to a narrower width, rows rendered
// at the new (narrower) geometry must emit \x1b[K so stale cells from the
// previous wider layout are erased. The erase fires only when the pane
// extends to the terminal's right edge (PaneRightEdge::TerminalEdge).
#[test]
fn resize_shrink_emits_erase_to_eol_for_terminal_edge_pane() {
    let grid = grid_with(2, 20, b"hello world twenty!!"); // fills the full 20-col row

    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();
    // First render at 20 cols to populate the cache.
    cache.render_full_snapshot(
        snapshot(&grid, 2, 20),
        0,
        0,
        2,
        20,
        PaneBodyDim::Normal,
        PaneRightEdge::TerminalEdge,
        &mut buf,
    );
    buf.clear();

    // Simulate resize to 10 cols: snapshot at the narrower width.  The
    // narrow screen would only show "hello worl" (first 10 chars).
    // render_full_snapshot (the cache falls back to full on geometry
    // change) must emit \x1b[K after each row so the stale right 10 cols
    // are cleared.
    let grid2 = grid_with(2, 10, b"hello worl");

    let stats = cache.render_full_snapshot(
        snapshot(&grid2, 2, 10),
        0,
        0,
        2,
        10,
        PaneBodyDim::Normal,
        PaneRightEdge::TerminalEdge,
        &mut buf,
    );
    let s = String::from_utf8_lossy(&buf);
    assert_eq!(stats.mode, PaneBodyRenderMode::Full);
    // Each rendered row must contain \x1b[K (erase to EOL) for terminal-edge panes.
    let erase_count = s.matches("\x1b[K").count();
    assert!(
        erase_count >= 1,
        "expected \\x1b[K in narrow-pane output to clear stale right-edge cells; got: {s:?}"
    );
}

#[test]
fn interior_pane_does_not_emit_erase_to_eol() {
    let grid = grid_with(2, 10, b"hello");
    let mut cache = PaneBodyCache::default();
    let mut buf = Vec::new();
    cache.render_full_snapshot(
        snapshot(&grid, 2, 10),
        0,
        0,
        2,
        10,
        PaneBodyDim::Normal,
        PaneRightEdge::Interior,
        &mut buf,
    );
    let s = String::from_utf8_lossy(&buf);
    assert!(
        !s.contains("\x1b[K"),
        "interior pane must not emit \\x1b[K to avoid clobbering adjacent pane content"
    );
}
