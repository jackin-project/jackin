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

    assert_eq!(patch.rows_changed.len(), 1);
    assert_eq!(patch.rows_changed[0].0, 1);
    let text: String = patch.rows_changed[0]
        .1
        .iter()
        .map(|cell| {
            if cell.text.is_empty() {
                " ".to_owned()
            } else {
                cell.text.clone()
            }
        })
        .collect();
    assert!(text.starts_with("gamma"));
    assert!(g.dump_dirty_patch().is_empty());
}
