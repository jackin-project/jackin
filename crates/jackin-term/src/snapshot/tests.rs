use super::*;

#[test]
fn snap_cell_blank_detection() {
    let c = SnapCell {
        text: String::new(),
        is_wide: false,
        is_wide_continuation: false,
        fg: Color::Default,
        bg: Color::Default,
        bold: false,
        italic: false,
        underline: false,
        underline_style: UnderlineStyle::None,
        underline_color: Color::Default,
        inverse: false,
        dim: false,
        strikethrough: false,
        slow_blink: false,
        rapid_blink: false,
        conceal: false,
        overline: false,
        hyperlink_id: None,
        hyperlink_uri: None,
    };
    assert!(c.is_blank());
    assert!(!c.has_contents());
}

#[test]
fn snap_cell_non_blank() {
    let c = SnapCell {
        text: "A".to_owned(),
        is_wide: false,
        is_wide_continuation: false,
        fg: Color::Default,
        bg: Color::Default,
        bold: false,
        italic: false,
        underline: false,
        underline_style: UnderlineStyle::None,
        underline_color: Color::Default,
        inverse: false,
        dim: false,
        strikethrough: false,
        slow_blink: false,
        rapid_blink: false,
        conceal: false,
        overline: false,
        hyperlink_id: None,
        hyperlink_uri: None,
    };
    assert!(!c.is_blank());
    assert!(c.has_contents());
}

#[test]
fn grid_snapshot_to_text_strips_trailing_spaces() {
    use crate::grid::DamageGrid;
    let mut grid = DamageGrid::new(3, 10, 1000);
    grid.process(b"Hello\r\nWorld");
    let snap = grid.dump();
    let text = snap.to_text();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "Hello");
    assert_eq!(lines[1], "World");
    // Row 3 is blank — trimmed to empty string.
    assert_eq!(lines.get(2), None);
}

#[test]
fn grid_snapshot_cursor_position() {
    use crate::grid::DamageGrid;
    let mut grid = DamageGrid::new(24, 80, 1000);
    grid.process(b"\x1b[5;10H"); // Move cursor to row 5, col 10 (1-based)
    let snap = grid.dump();
    assert_eq!(snap.cursor, (4, 9)); // 0-based
}

#[test]
fn grid_snapshot_wide_chars_in_text() {
    use crate::grid::DamageGrid;
    let mut grid = DamageGrid::new(3, 10, 1000);
    grid.process("你好".as_bytes());
    let snap = grid.dump();
    let text = snap.to_text();
    assert!(text.starts_with("你好"));
}

#[test]
fn grid_snapshot_non_blank_count() {
    use crate::grid::DamageGrid;
    let mut grid = DamageGrid::new(3, 10, 1000);
    grid.process(b"ABC");
    let snap = grid.dump();
    assert_eq!(snap.non_blank_count(), 3);
}

#[test]
fn grid_patch_counts_changed_rows_and_cells() {
    use crate::grid::DamageGrid;
    let mut grid = DamageGrid::new(3, 10, 1000);
    drop(grid.dump_dirty_patch());

    grid.process(b"\x1b[2;1HABC");
    let patch = grid.dump_dirty_patch();

    assert_eq!(patch.changed_row_count(), 1);
    assert_eq!(patch.changed_cell_count(), 3);
    assert_eq!(
        patch
            .changed_spans()
            .map(|(row, start, cells)| (row, start, cells.len()))
            .collect::<Vec<_>>(),
        [(1, 0, 3)]
    );
}
