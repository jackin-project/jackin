//! Tests for `compositor`.
use jackin_term::{DamageGrid, UnderlineStyle};

use super::{SgrMetadata, pane_sgr_regions};
use crate::tui::app::VisiblePane;
use crate::tui::layout::Rect;
use crate::tui::view::PaneScreen;

/// Run `bytes` through `pane_sgr_regions` for a single pane with the given
/// `inner` area, so callers can vary the inner offset/size to exercise the
/// pane-relative offset arithmetic and the `inner`-vs-`view` clamp.
fn sgr_regions_for_inner(bytes: &[u8], inner: Rect) -> Vec<(ratatui::layout::Rect, SgrMetadata)> {
    let mut grid = DamageGrid::new(2, 10, 100);
    grid.process(bytes);
    let view = grid.scrollback_view(0, 2);
    let panes = vec![VisiblePane {
        id: 1,
        outer: inner,
        inner,
        focused: false,
    }];
    let pane_screens = vec![(1u64, PaneScreen::View(view))];
    pane_sgr_regions(&panes, &pane_screens)
}

/// Standard helper: inner offset by (row 2, col 3) so returned rects are
/// pane-relative, not raw grid coordinates.
fn sgr_regions_for(bytes: &[u8]) -> Vec<(ratatui::layout::Rect, SgrMetadata)> {
    sgr_regions_for_inner(bytes, Rect::new(2, 3, 5, 10))
}

#[test]
fn pane_sgr_regions_coalesces_one_styled_run_and_skips_default() {
    // Curly underline on "ab", then plain "cd": one region over the two
    // styled cells, offset into the pane's inner area; the default-SGR tail
    // contributes nothing.
    let regions = sgr_regions_for(b"\x1b[4:3mab\x1b[24mcd");
    assert_eq!(regions.len(), 1, "got {regions:?}");
    let (rect, metadata) = regions[0];
    assert_eq!((rect.x, rect.y, rect.width, rect.height), (3, 2, 2, 1));
    assert_eq!(metadata.underline_style, UnderlineStyle::Curly);
}

#[test]
fn pane_sgr_regions_splits_adjacent_differing_runs() {
    // Curly "ab" then double-underline "cd": two separate regions, not one
    // merged span — the run breaks when the probed metadata differs.
    let regions = sgr_regions_for(b"\x1b[4:3mab\x1b[4:2mcd");
    assert_eq!(regions.len(), 2, "got {regions:?}");
    assert_eq!(regions[0].0.x, 3);
    assert_eq!(regions[0].0.width, 2);
    assert_eq!(regions[0].1.underline_style, UnderlineStyle::Curly);
    assert_eq!(regions[1].0.x, 5);
    assert_eq!(regions[1].0.width, 2);
    assert_eq!(regions[1].1.underline_style, UnderlineStyle::Double);
}

#[test]
fn pane_sgr_regions_empty_when_nothing_styled() {
    assert!(sgr_regions_for(b"plain text").is_empty());
}

#[test]
fn pane_sgr_regions_clamps_run_to_inner_width() {
    // A styled run spanning grid cols 0..6, but the pane's inner area is only
    // 4 cols wide: the emitted run must be clamped to the pane, not escape it.
    let regions = sgr_regions_for_inner(b"\x1b[4:3mabcdef", Rect::new(0, 0, 5, 4));
    assert_eq!(regions.len(), 1, "got {regions:?}");
    assert_eq!(regions[0].0.width, 4, "run must clamp to inner cols");
    assert_eq!(regions[0].1.underline_style, UnderlineStyle::Curly);
}
