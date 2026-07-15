// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Cell width + row helpers extracted from grid.rs.
#[cfg_attr(
    not(test),
    expect(clippy::wildcard_imports, reason = "target-dependent")
)]
use super::*;

/// Display width of a cell: 2 for wide lead, 1 for a filled narrow cell, else 0.
pub fn cell_width(cell: &Cell) -> u16 {
    if cell.is_wide {
        2
    } else {
        u16::from(!(cell.is_wide_continuation || cell.contents.is_empty()))
    }
}

/// Set lead/continuation wide-cell flags for `row[col]` after writing a glyph.
pub fn set_cell_width(row: &mut [Cell], col: usize, width: u16, attrs: Attrs, cols: usize) {
    row[col].is_wide = width > 1;
    row[col].is_wide_continuation = false;

    if col + 1 < cols && col + 1 < row.len() {
        if width > 1 {
            let hyperlink = row[col].hyperlink.clone();
            let hyperlink_id = row[col].hyperlink_id;
            row[col + 1] = Cell {
                contents: compact_str::CompactString::new(""),
                is_wide: false,
                is_wide_continuation: true,
                attrs,
                hyperlink_id,
                hyperlink,
            };
        } else if row[col + 1].is_wide_continuation {
            row[col + 1] = Cell::default();
        }
    }
}

// ── Grid construction helpers ─────────────────────────────────────────────

/// Allocate a row of default (blank) cells.
pub fn blank_row(cols: u16) -> Vec<Cell> {
    vec![Cell::default(); cols as usize]
}

/// Build a blank `RowStore` of `rows` × `cols` using `arena` for row reuse.
pub fn make_blank_grid(rows: u16, cols: u16, arena: RowArena) -> RowStore {
    RowStore::blank(rows, cols, arena)
}

/// Length of a trailing incomplete UTF-8 sequence that should be buffered.
pub fn incomplete_utf8_suffix_len(bytes: &[u8]) -> usize {
    let Some(last) = bytes.last() else {
        return 0;
    };
    if last.is_ascii() {
        return 0;
    }

    let start = bytes
        .iter()
        .rposition(u8::is_ascii)
        .map_or(0, |idx| idx + 1);
    let suffix = &bytes[start..];
    match std::str::from_utf8(suffix) {
        Ok(_) => 0,
        Err(err) if err.valid_up_to() > 0 => suffix.len() - err.valid_up_to(),
        Err(_) => suffix.len(),
    }
}
