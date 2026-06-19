use super::DamageGrid;

#[test]
fn dump_scrollback_view_offset_zero_matches_live() {
    let mut g = DamageGrid::new(2, 8, 100);
    g.process(b"abc\r\ndef\r\nghi");
    let view = g.dump_scrollback_view(0, 2);
    assert_eq!(view.cells, g.dump().cells);
}

#[test]
fn dump_scrollback_view_shows_scrolled_history() {
    let mut g = DamageGrid::new(2, 8, 100);
    for i in 0..6 {
        g.process(format!("line{i}\r\n").as_bytes());
    }
    assert!(g.scrollback_len() > 0, "scrollback should have filled");
    let view = g.dump_scrollback_view(2, 2);
    assert_eq!(view.cells.len(), 2);
    // The scrolled-up view differs from the live tail; it shows history.
    assert_ne!(view.cells, g.dump().cells);
}

#[test]
fn borrowed_scrollback_view_matches_owned_dump() {
    let mut g = DamageGrid::new(3, 10, 100);
    for i in 0..8 {
        g.process(format!("line{i}\r\n").as_bytes());
    }

    let owned = g.dump_scrollback_view(2, 3);
    let borrowed = g.scrollback_view(2, 3);

    assert_eq!(borrowed.rows, owned.rows);
    assert_eq!(borrowed.cols, owned.cols);
    for row in 0..owned.rows {
        for col in 0..owned.cols {
            let owned_cell = owned.cell(row, col).expect("owned cell");
            let borrowed_cell = borrowed.cell(row, col).expect("borrowed cell");
            assert_eq!(borrowed_cell.contents(), owned_cell.text);
            assert_eq!(borrowed_cell.is_wide, owned_cell.is_wide);
            assert_eq!(
                borrowed_cell.is_wide_continuation,
                owned_cell.is_wide_continuation
            );
            assert_eq!(borrowed_cell.fgcolor(), owned_cell.fg);
            assert_eq!(borrowed_cell.bgcolor(), owned_cell.bg);
            assert_eq!(borrowed_cell.bold(), owned_cell.bold);
            assert_eq!(borrowed_cell.italic(), owned_cell.italic);
            assert_eq!(borrowed_cell.underline(), owned_cell.underline);
            assert_eq!(borrowed_cell.inverse(), owned_cell.inverse);
            assert_eq!(borrowed_cell.dim(), owned_cell.dim);
        }
    }
}

#[test]
fn zero_scrollback_limit_evicts_without_panic() {
    // scrollback_limit == 0 means "no scrollback"; rows that would be
    // preserved must be dropped, not pushed onto an empty buffer with a
    // remove(0) that would panic (len 0).
    let mut g = DamageGrid::new(2, 8, 0);
    for i in 0..6 {
        g.process(format!("line{i}\r\n").as_bytes());
    }
    assert_eq!(g.scrollback_len(), 0);
}

#[test]
fn dump_dirty_patch_drains_changed_rows_only() {
    let mut g = DamageGrid::new(3, 12, 100);
    g.process(b"\x1b[1;1Halpha\x1b[2;1Hbeta");
    drop(g.dump_dirty_patch());

    g.process(b"\x1b[2;1Hgamma");
    let patch = g.dump_dirty_patch();
    let changed_rows = patch.changed_rows().collect::<Vec<_>>();

    assert_eq!(changed_rows.len(), 1);
    assert_eq!(changed_rows[0].0, 1);
    let text: String = changed_rows[0]
        .1
        .iter()
        .map(|cell| {
            if cell.contents().is_empty() {
                " ".to_owned()
            } else {
                cell.contents().to_owned()
            }
        })
        .collect();
    assert!(text.starts_with("gamma"));
    assert!(g.dump_dirty_patch().is_empty());
}

#[test]
fn dump_dirty_patch_tracks_changed_cell_span() {
    let mut g = DamageGrid::new(3, 12, 100);
    g.process(b"\x1b[1;1Halpha\x1b[2;1Hbeta");
    drop(g.dump_dirty_patch());

    g.process(b"\x1b[2;3HZ");
    let patch = g.dump_dirty_patch();

    assert_eq!(patch.changed_row_count(), 1);
    assert_eq!(patch.changed_cell_count(), 1);
    assert_eq!(
        patch
            .changed_spans()
            .map(|(row, start, cells)| (row, start, cells.len()))
            .collect::<Vec<_>>(),
        [(1, 2, 1)]
    );
}
