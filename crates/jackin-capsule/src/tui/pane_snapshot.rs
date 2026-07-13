// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Render a `DamageGrid` snapshot into the host terminal at a pane rectangle.
//! Cursor positioning is offset by the pane's origin so the agent's
//! `(0, 0)` lands at `(dest_row, dest_col)`.

use std::io::Write;
use std::ops::Range;

use jackin_tui::ansi::{RESET, fg};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellSnapshot {
    pub contents: String,
    pub width: u16,
}

/// Render/selection projection derived from `DamageGrid`.
///
/// This is not a second terminal model. It preserves cell widths for
/// scrollback text selection and operator-facing screen dumps until those
/// callers can consume `GridSnapshot` / dirty spans directly.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowSnapshot {
    pub cells: Vec<CellSnapshot>,
}

impl RowSnapshot {
    #[must_use]
    pub fn text_range(&self, from_col: u16, to_col: u16) -> String {
        let mut out = String::new();
        for cell in row_range_cells(self, from_col, to_col) {
            out.push_str(&cell.contents);
        }
        out
    }

    /// Each cell with its inclusive display-column range. Word-boundary
    /// walks need column geometry (wide cells span two columns) and the
    /// cell text together. Width-0 cells are dropped after the column
    /// accumulation: their inclusive range would be inverted (or alias
    /// column 0), and no display column maps to them.
    #[must_use]
    pub fn display_cells(&self) -> Vec<DisplayCell<'_>> {
        let mut col = 0u16;
        self.cells
            .iter()
            .filter_map(|cell| {
                let start_col = col;
                col = col.saturating_add(cell.width);
                (cell.width > 0).then_some(DisplayCell {
                    start_col,
                    end_col: col.saturating_sub(1),
                    contents: &cell.contents,
                })
            })
            .collect()
    }
}

/// A pane cell paired with the inclusive display columns it occupies.
#[derive(Clone, Copy, Debug)]
pub struct DisplayCell<'a> {
    pub start_col: u16,
    pub end_col: u16,
    pub contents: &'a str,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaneBodyDim {
    #[default]
    Normal,
    Inactive,
}

/// Paint the scrollbar thumb onto the pane's right border column
/// (`outer_col + outer_cols - 1`) on top of the box's `│` characters.
/// Only thumb rows are emitted: non-thumb rows keep the box border
/// underneath so the scrollbar reads as a textured border, not as a
/// duplicate vertical line. `filled == 0` suppresses the call
/// entirely so alternate-screen TUIs and fresh primary-screen panes
/// keep their full border.
///
/// Thumb height is proportional to viewport / total; thumb position
/// represents the slice of history the operator is looking at
/// (bottom row → live tail, top row → oldest scrollback line). This is the
/// PTY/DamageGrid tail-relative scrollbar exception: raw ANSI emission cannot
/// use ratatui's `FixedScrollbar`, but it still uses the shared
/// `tail_vertical_thumb` geometry.
/// Thumb colour is phosphor-green for focused panes, gray for the
/// rest — matches the surrounding border so focus and chrome
/// agree.
#[allow(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn draw_scrollbar(
    buf: &mut Vec<u8>,
    pane_row: u16,
    pane_col: u16,
    pane_rows: u16,
    pane_cols: u16,
    offset: usize,
    filled: usize,
    focused: bool,
) {
    // Bail on zero-width / zero-height panes before doing any
    // arithmetic. `pane_col + pane_cols - 1` would underflow u16 when
    // pane_cols == 0; saturating_add+saturating_sub keep arithmetic
    // total even for runaway resize ticks where pane geometry briefly
    // hits zero.
    if pane_cols == 0 || pane_rows < 2 {
        return;
    }
    // Constrain the track to the pane's interior rows so the
    // top-right `┐` and bottom-right `┘` corners stay intact. Without
    // this guard the thumb overwrote one of the corners whenever the
    // scrollback was at the live tail (or the top), producing the
    // visible "scrollbar sticks out past the pane" symptom.
    let interior_rows = pane_rows.saturating_sub(2);
    let Some(thumb) = jackin_tui::scroll::tail_vertical_thumb(interior_rows, filled, offset) else {
        return;
    };
    let col = pane_col.saturating_add(pane_cols).saturating_sub(1);

    // Active pane uses the brand phosphor-green; inactive panes a
    // neutral gray that matches their inactive border colour.
    let thumb_color = if focused {
        jackin_tui::PHOSPHOR_GREEN
    } else {
        jackin_tui::Rgb::new(160, 160, 160)
    };

    // Thumb rows are 0-based relative to the interior; skip the top
    // border row by adding 1 to `pane_row`.
    let track_start_row = pane_row + 1;
    for r in 0..thumb.len {
        let _unused = write!(
            buf,
            "\x1b[{};{}H",
            track_start_row + thumb.start + r + 1,
            col + 1
        );
        buf.extend_from_slice(RESET.as_bytes());
        fg(buf, thumb_color);
        buf.extend_from_slice("█".as_bytes());
    }
    buf.extend_from_slice(RESET.as_bytes());
}

/// Snapshot a single row from a `DamageGrid` into a `RowSnapshot`.
#[must_use]
pub fn snapshot_damagegrid_row(
    grid: &jackin_term::DamageGrid,
    row: u16,
    cols_to_draw: u16,
) -> RowSnapshot {
    let mut cells = Vec::with_capacity(cols_to_draw as usize);
    let mut col = 0;
    while col < cols_to_draw {
        let cell = grid.cell(row, col);
        if cell.is_some_and(|c| c.is_wide_continuation) {
            col += 1;
            continue;
        }

        let width = cell
            .filter(|c| c.is_wide)
            .map_or(1, |_| 2)
            .min(cols_to_draw - col);
        let contents = match cell {
            Some(c) if c.has_contents() => c.contents().to_owned(),
            _ => " ".repeat(width as usize),
        };
        cells.push(CellSnapshot { contents, width });
        col += width;
    }
    RowSnapshot { cells }
}

/// Build a content-coordinate snapshot: retained scrollback rows oldest-first,
/// followed by the current live screen. Selection copy uses this so a range can
/// span outside the currently visible viewport.
#[must_use]
pub fn pane_content_from_damagegrid(
    grid: &jackin_term::DamageGrid,
    viewport_cols: u16,
) -> Vec<RowSnapshot> {
    let (screen_rows, _screen_cols) = grid.size();
    let filled = grid.scrollback_len();
    let total = filled.saturating_add(usize::from(screen_rows));
    pane_content_range_from_damagegrid(grid, viewport_cols, 0..total)
}

/// Content-coordinate snapshot of a half-open row range.
///
/// Index 0 of the returned vec is content row `content_rows.start` (after
/// clamping). Callers that need absolute content coordinates keep the range
/// start and map `rows[i]` → content row `content_rows.start + i`.
///
/// Used by per-mouse-event link resolution so a single anchor row does not
/// materialize the entire retained scrollback (up to the 10k-row bound).
#[must_use]
pub fn pane_content_range_from_damagegrid(
    grid: &jackin_term::DamageGrid,
    viewport_cols: u16,
    content_rows: Range<usize>,
) -> Vec<RowSnapshot> {
    let (screen_rows, screen_cols) = grid.size();
    let cols_to_draw = viewport_cols.min(screen_cols);
    let filled = grid.scrollback_len();
    let total = filled.saturating_add(usize::from(screen_rows));

    let start = content_rows.start.min(total);
    let end = content_rows.end.min(total);
    if start >= end {
        return Vec::new();
    }

    let mut snapshot = Vec::with_capacity(end - start);

    // Scrollback portion: content indices [0, filled) map 1:1 onto the
    // oldest-first scrollback store. `scrollback_rows_at_offset(offset, n)`
    // returns rows starting at store index `filled - offset` — request
    // offset = filled - sb_start so the first returned row is content `sb_start`.
    let sb_start = start.min(filled);
    let sb_end = end.min(filled);
    if sb_start < sb_end {
        let offset_from_tail = filled.saturating_sub(sb_start);
        let max_rows = sb_end - sb_start;
        for sb_row in grid.scrollback_rows_at_offset(offset_from_tail, max_rows) {
            snapshot.push(snapshot_damagegrid_cells(sb_row, cols_to_draw));
        }
    }

    // Live-screen portion: content indices [filled, filled + screen_rows).
    let screen_start = start.saturating_sub(filled);
    let screen_end = end.saturating_sub(filled);
    let screen_limit = usize::from(screen_rows);
    let screen_start = screen_start.min(screen_limit);
    let screen_end = screen_end.min(screen_limit);
    for row in screen_start..screen_end {
        let row_u16 = u16::try_from(row).unwrap_or(u16::MAX);
        snapshot.push(snapshot_damagegrid_row(grid, row_u16, cols_to_draw));
    }

    snapshot
}

/// Build a `RowSnapshot` from a raw slice of `jackin_term::Cell`s.
fn snapshot_damagegrid_cells(cells: &[jackin_term::Cell], cols_to_draw: u16) -> RowSnapshot {
    let mut out = Vec::with_capacity(cols_to_draw as usize);
    let mut col = 0u16;
    for cell in cells {
        if col >= cols_to_draw {
            break;
        }
        if cell.is_wide_continuation {
            col += 1;
            continue;
        }
        let width = if cell.is_wide { 2 } else { 1 }.min(cols_to_draw - col);
        let contents = if cell.has_contents() {
            cell.contents().to_owned()
        } else {
            " ".repeat(width as usize)
        };
        out.push(CellSnapshot { contents, width });
        col += width;
    }
    RowSnapshot { cells: out }
}

fn row_range_cells(row: &RowSnapshot, from_col: u16, to_col: u16) -> Vec<CellSnapshot> {
    if to_col < from_col {
        return Vec::new();
    }
    let mut cells = Vec::new();
    let mut col = 0u16;
    for cell in &row.cells {
        let cell_start = col;
        let cell_end = col.saturating_add(cell.width).saturating_sub(1);
        col = col.saturating_add(cell.width);
        if cell_end < from_col {
            continue;
        }
        if cell_start > to_col {
            break;
        }
        if cell_start >= from_col && cell_end <= to_col {
            cells.push(cell.clone());
            continue;
        }
        let overlap_start = cell_start.max(from_col);
        let overlap_end = cell_end.min(to_col);
        let width = overlap_end.saturating_sub(overlap_start).saturating_add(1);
        cells.push(CellSnapshot {
            contents: " ".repeat(usize::from(width)),
            width,
        });
    }
    cells
}

#[cfg(test)]
mod tests;
