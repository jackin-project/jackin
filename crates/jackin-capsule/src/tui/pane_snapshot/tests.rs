//! Equivalence tests: ranged content snapshots match slices of the full snapshot.

use jackin_term::DamageGrid;

use super::{pane_content_from_damagegrid, pane_content_range_from_damagegrid};

/// Feed enough line feeds to push `scrollback_rows` into history, then a
/// live-screen marker on the final visible line.
fn grid_with_scrollback(screen_rows: u16, cols: u16, scrollback_rows: usize) -> DamageGrid {
    let mut grid = DamageGrid::new(screen_rows, cols, scrollback_rows.saturating_add(64));
    // Each `\n` advances a row; write a unique digit so rows are distinguishable.
    for i in 0..scrollback_rows.saturating_add(usize::from(screen_rows)) {
        let ch = char::from(b'A' + (i % 26) as u8);
        let line = format!("{ch}{i:04}\r\n");
        grid.process(line.as_bytes());
    }
    grid
}

#[test]
fn ranged_equals_full_slice_entirely_in_scrollback() {
    let grid = grid_with_scrollback(4, 20, 12);
    let full = pane_content_from_damagegrid(&grid, 20);
    let filled = grid.scrollback_len();
    assert!(filled >= 8, "expected deep scrollback, filled={filled}");

    let range = 2..6;
    let ranged = pane_content_range_from_damagegrid(&grid, 20, range.clone());
    assert_eq!(ranged, full[range].to_vec());
}

#[test]
fn ranged_equals_full_slice_spanning_scrollback_live_boundary() {
    let grid = grid_with_scrollback(4, 20, 8);
    let full = pane_content_from_damagegrid(&grid, 20);
    let filled = grid.scrollback_len();
    assert!(filled > 0);
    assert_eq!(full.len(), filled + 4);

    let start = filled.saturating_sub(2);
    let end = filled + 2;
    let ranged = pane_content_range_from_damagegrid(&grid, 20, start..end);
    assert_eq!(ranged, full[start..end].to_vec());
}

#[test]
fn ranged_equals_full_slice_entirely_in_live_screen() {
    let grid = grid_with_scrollback(5, 16, 6);
    let full = pane_content_from_damagegrid(&grid, 16);
    let filled = grid.scrollback_len();
    let range = filled..filled + 3;
    let ranged = pane_content_range_from_damagegrid(&grid, 16, range.clone());
    assert_eq!(ranged, full[range].to_vec());
}

#[test]
fn ranged_out_of_bounds_clamps() {
    let grid = grid_with_scrollback(3, 12, 4);
    let full = pane_content_from_damagegrid(&grid, 12);
    let total = full.len();

    // Past the end → empty.
    assert!(pane_content_range_from_damagegrid(&grid, 12, total..total + 10).is_empty());
    // Inverted / empty range → empty.
    assert!(pane_content_range_from_damagegrid(&grid, 12, 5..5).is_empty());
    assert!(pane_content_range_from_damagegrid(&grid, 12, 8..3).is_empty());
    // Start in range, end past total → truncated to full[start..].
    let start = total.saturating_sub(2);
    let ranged = pane_content_range_from_damagegrid(&grid, 12, start..total + 50);
    assert_eq!(ranged, full[start..].to_vec());
}

#[test]
fn ranged_empty_scrollback_live_only() {
    let mut grid = DamageGrid::new(3, 10, 100);
    grid.process(b"abc\r\n");
    assert_eq!(grid.scrollback_len(), 0);

    let full = pane_content_from_damagegrid(&grid, 10);
    assert_eq!(full.len(), 3);
    let ranged = pane_content_range_from_damagegrid(&grid, 10, 0..2);
    assert_eq!(ranged, full[0..2].to_vec());
}

#[test]
fn ranged_matches_full_for_seeded_window_sizes() {
    // Property-style: for a few fixed grids and window starts, range == full[r].
    for (screen, cols, sb, start, len) in [
        (4u16, 20u16, 10usize, 0usize, 3usize),
        (4, 20, 10, 5, 4),
        (4, 20, 10, 8, 6), // crosses boundary
        (6, 12, 0, 1, 2),  // no scrollback
        (3, 8, 15, 12, 1), // single row (hover window size)
    ] {
        let grid = grid_with_scrollback(screen, cols, sb);
        let full = pane_content_from_damagegrid(&grid, cols);
        let end = (start + len).min(full.len());
        let start = start.min(full.len());
        let ranged = pane_content_range_from_damagegrid(&grid, cols, start..end);
        assert_eq!(
            ranged,
            full[start..end].to_vec(),
            "mismatch for screen={screen} cols={cols} sb={sb} range={start}..{end}"
        );
    }
}
