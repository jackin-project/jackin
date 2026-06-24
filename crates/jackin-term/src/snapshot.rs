//! `GridSnapshot` — complete screen state capture for testing and terminal observation.
//!
//! `DamageGrid::dump()` serializes the current screen into a `GridSnapshot` so
//! acceptance tests can assert exact screen state and external observers can
//! read the terminal contents without going through the vte/ANSI encode path.
//!
//! The name and concept are borrowed from `avt` (MIT licensed, by Marcin Kulik):
//! <https://github.com/asciinema/avt>. The implementation is our own — avt is
//! not a dependency.
//!
//! # Attribution
//! The `dump()` / snapshot-for-observation pattern: avt (MIT), Marcin Kulik /
//! asciinema project.

use std::collections::vec_deque;

use crate::{
    cell::{Attrs, Cell, Color},
    damage::{DirtySpan, DirtySpans},
    grid::{RowStore, RowWrap},
};

/// A snapshot of a single cell at dump time.
///
/// All fields are owned so the snapshot is independent of the grid's lifetime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapCell {
    /// The grapheme cluster at this position. Empty string = blank/space.
    pub text: String,
    /// True for the lead column of a wide (2-col) character.
    pub is_wide: bool,
    /// True for the continuation column of a wide character.
    pub is_wide_continuation: bool,
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underline.
    pub underline: bool,
    /// Reverse video.
    pub inverse: bool,
    /// Dim / faint.
    pub dim: bool,
}

impl SnapCell {
    /// True if this cell contains a non-blank grapheme.
    pub fn has_contents(&self) -> bool {
        !self.text.is_empty()
    }

    /// True if this cell is blank (empty text, default attrs).
    pub fn is_blank(&self) -> bool {
        self.text.is_empty() && self.attrs_are_default()
    }

    fn attrs_are_default(&self) -> bool {
        self.fg == Color::Default
            && self.bg == Color::Default
            && !self.bold
            && !self.italic
            && !self.underline
            && !self.inverse
            && !self.dim
    }
}

impl From<&Cell> for SnapCell {
    fn from(cell: &Cell) -> Self {
        Self {
            text: cell.contents().to_owned(),
            is_wide: cell.is_wide,
            is_wide_continuation: cell.is_wide_continuation,
            fg: cell.fgcolor(),
            bg: cell.bgcolor(),
            bold: cell.bold(),
            italic: cell.italic(),
            underline: cell.underline(),
            inverse: cell.inverse(),
            dim: cell.dim(),
        }
    }
}

/// A complete snapshot of one screen (primary or alternate).
///
/// Rows are in display order (row 0 = top). Columns are in display order.
/// This is the value type for testing and terminal observation.
#[derive(Debug, Clone)]
pub struct GridSnapshot {
    /// Number of rows.
    pub rows: u16,
    /// Number of columns.
    pub cols: u16,
    /// Cursor position `(row, col)` at snapshot time.
    pub cursor: (u16, u16),
    /// Whether the alternate screen was active at snapshot time.
    pub alternate_screen: bool,
    /// The cell grid in row-major order.
    pub cells: Vec<Vec<SnapCell>>,
    /// Row wrap provenance in row-major order.
    pub row_wraps: Vec<RowWrap>,
}

/// Borrowed terminal view for render paths that do not need an owned snapshot.
#[derive(Debug)]
pub struct GridView<'a> {
    /// Number of rows.
    pub rows: u16,
    /// Number of columns.
    pub cols: u16,
    /// Cursor position `(row, col)` at view creation time.
    pub cursor: (u16, u16),
    /// Whether the alternate screen was active at view creation time.
    pub alternate_screen: bool,
    screen: &'a RowStore,
    scrollback: &'a RowStore,
    scrollback_start: usize,
    scrollback_prefix: usize,
}

impl<'a> GridView<'a> {
    pub(crate) const fn new(parts: GridViewParts<'a>) -> Self {
        Self {
            rows: parts.rows,
            cols: parts.cols,
            cursor: parts.cursor,
            alternate_screen: parts.alternate_screen,
            screen: parts.screen,
            scrollback: parts.scrollback,
            scrollback_start: parts.scrollback_start,
            scrollback_prefix: parts.scrollback_prefix,
        }
    }

    /// Return the cell at `(row, col)`, or `None` if out of bounds.
    #[must_use]
    pub fn cell(&self, row: u16, col: u16) -> Option<&'a Cell> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        let row_idx = usize::from(row);
        let col_idx = usize::from(col);
        if row_idx < self.scrollback_prefix {
            return self
                .scrollback
                .get(self.scrollback_start + row_idx)
                .and_then(|r| r.get(col_idx));
        }
        self.screen
            .get(row_idx - self.scrollback_prefix)
            .and_then(|r| r.get(col_idx))
    }

    /// Return row wrap provenance for a visible row.
    #[must_use]
    pub fn row_wrap(&self, row: u16) -> Option<RowWrap> {
        if row >= self.rows {
            return None;
        }
        let row_idx = usize::from(row);
        if row_idx < self.scrollback_prefix {
            return self.scrollback.wrap(self.scrollback_start + row_idx);
        }
        self.screen.wrap(row_idx - self.scrollback_prefix)
    }
}

#[derive(Debug)]
pub(crate) struct GridViewParts<'a> {
    pub(crate) rows: u16,
    pub(crate) cols: u16,
    pub(crate) cursor: (u16, u16),
    pub(crate) alternate_screen: bool,
    pub(crate) screen: &'a RowStore,
    pub(crate) scrollback: &'a RowStore,
    pub(crate) scrollback_start: usize,
    pub(crate) scrollback_prefix: usize,
}

/// Borrowed view of rows changed since the last dirty-span drain.
#[derive(Debug)]
pub struct GridPatch<'a> {
    /// Number of rows in the full grid.
    pub rows: u16,
    /// Number of columns in the full grid.
    pub cols: u16,
    /// Cursor position `(row, col)` at snapshot time.
    pub cursor: (u16, u16),
    /// Whether the alternate screen was active at snapshot time.
    pub alternate_screen: bool,
    /// The active screen at patch creation time.
    screen: &'a RowStore,
    /// Changed rows in display order.
    dirty: DirtySpans,
}

impl<'a> GridPatch<'a> {
    pub(crate) const fn new(
        rows: u16,
        cols: u16,
        cursor: (u16, u16),
        alternate_screen: bool,
        screen: &'a RowStore,
        dirty: DirtySpans,
    ) -> Self {
        Self {
            rows,
            cols,
            cursor,
            alternate_screen,
            screen,
            dirty,
        }
    }

    /// True when no rows changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match &self.dirty {
            DirtySpans::All => self.screen.is_empty(),
            DirtySpans::Rows(rows) => rows.is_empty(),
        }
    }

    /// Number of rows this patch will emit.
    #[must_use]
    pub fn changed_row_count(&self) -> usize {
        match &self.dirty {
            DirtySpans::All => self.screen.len(),
            DirtySpans::Rows(rows) => rows.len(),
        }
    }

    /// Number of grid cells covered by this patch.
    #[must_use]
    pub fn changed_cell_count(&self) -> usize {
        match &self.dirty {
            DirtySpans::All => self.screen.len() * usize::from(self.cols),
            DirtySpans::Rows(rows) => rows.iter().map(|span| span_width(span, self.cols)).sum(),
        }
    }

    /// Return the changed row at `row`, or `None` when that row is unchanged.
    #[must_use]
    pub fn row(&self, row: u16) -> Option<&'a [Cell]> {
        match &self.dirty {
            DirtySpans::All => self.screen.get(row as usize).map(Vec::as_slice),
            DirtySpans::Rows(rows) => rows
                .contains(row)
                .then(|| self.screen.get(row as usize).map(Vec::as_slice))
                .flatten(),
        }
    }

    /// Iterate changed rows in display order without allocating row snapshots.
    pub fn changed_rows(&self) -> ChangedRows<'a, '_> {
        match &self.dirty {
            DirtySpans::All => ChangedRows {
                screen: self.screen,
                state: ChangedRowsState::All(self.screen.iter().enumerate()),
            },
            DirtySpans::Rows(rows) => ChangedRows {
                screen: self.screen,
                state: ChangedRowsState::Rows(rows.as_slice().iter()),
            },
        }
    }

    /// Iterate changed row spans in display order without allocating snapshots.
    pub fn changed_spans(&self) -> ChangedSpans<'a, '_> {
        match &self.dirty {
            DirtySpans::All => ChangedSpans {
                screen: self.screen,
                cols: self.cols,
                state: ChangedSpansState::All(self.screen.iter().enumerate()),
            },
            DirtySpans::Rows(rows) => ChangedSpans {
                screen: self.screen,
                cols: self.cols,
                state: ChangedSpansState::Rows(rows.as_slice().iter()),
            },
        }
    }
}

fn span_width(span: DirtySpan, cols: u16) -> usize {
    let end = span.end_col.min(cols);
    usize::from(end.saturating_sub(span.start_col.min(end)))
}

#[derive(Debug)]
pub struct ChangedRows<'a, 'dirty> {
    screen: &'a RowStore,
    state: ChangedRowsState<'a, 'dirty>,
}

#[derive(Debug)]
enum ChangedRowsState<'a, 'dirty> {
    All(std::iter::Enumerate<vec_deque::Iter<'a, Vec<Cell>>>),
    Rows(std::slice::Iter<'dirty, DirtySpan>),
}

impl<'a> Iterator for ChangedRows<'a, '_> {
    type Item = (u16, &'a [Cell]);

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.state {
            ChangedRowsState::All(rows) => {
                rows.next().map(|(idx, row)| (idx as u16, row.as_slice()))
            }
            ChangedRowsState::Rows(rows) => {
                for span in rows.by_ref() {
                    if let Some(row) = self.screen.get(span.row as usize) {
                        return Some((span.row, row.as_slice()));
                    }
                }
                None
            }
        }
    }
}

#[derive(Debug)]
pub struct ChangedSpans<'a, 'dirty> {
    screen: &'a RowStore,
    cols: u16,
    state: ChangedSpansState<'a, 'dirty>,
}

#[derive(Debug)]
enum ChangedSpansState<'a, 'dirty> {
    All(std::iter::Enumerate<vec_deque::Iter<'a, Vec<Cell>>>),
    Rows(std::slice::Iter<'dirty, DirtySpan>),
}

impl<'a> Iterator for ChangedSpans<'a, '_> {
    type Item = (u16, u16, &'a [Cell]);

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.state {
            ChangedSpansState::All(rows) => rows.next().map(|(idx, row)| {
                let end = usize::from(self.cols.min(row.len() as u16));
                (idx as u16, 0, &row[..end])
            }),
            ChangedSpansState::Rows(spans) => {
                for span in spans.by_ref() {
                    let Some(row) = self.screen.get(span.row as usize) else {
                        continue;
                    };
                    let start = usize::from(span.start_col.min(self.cols));
                    let end = usize::from(span.end_col.min(self.cols));
                    if start >= end || start >= row.len() {
                        continue;
                    }
                    let end = end.min(row.len());
                    return Some((span.row, span.start_col, &row[start..end]));
                }
                None
            }
        }
    }
}

impl GridSnapshot {
    /// Return a human-readable text representation of the grid contents.
    ///
    /// Wide-char continuation cells are collapsed (the continuation column is
    /// not re-printed), so the output is the visual text a human would read.
    /// Trailing blank cells on each row are stripped.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (i, row) in self.cells.iter().enumerate() {
            let mut row_text = String::new();
            for cell in row {
                if cell.is_wide_continuation {
                    // Skip — the wide char is already in the lead column.
                    continue;
                }
                if cell.text.is_empty() {
                    row_text.push(' ');
                } else {
                    row_text.push_str(&cell.text);
                }
            }
            // Strip trailing spaces.
            let trimmed = row_text.trim_end();
            out.push_str(trimmed);
            if i + 1 < self.cells.len() {
                out.push('\n');
            }
        }
        out
    }

    /// Return the cell at `(row, col)`, or `None` if out of bounds.
    pub fn cell(&self, row: u16, col: u16) -> Option<&SnapCell> {
        self.cells
            .get(row as usize)
            .and_then(|r| r.get(col as usize))
    }

    /// Count non-blank cells across the whole grid.
    pub fn non_blank_count(&self) -> usize {
        self.cells
            .iter()
            .flat_map(|row| row.iter())
            .filter(|c| c.has_contents())
            .count()
    }
}

/// Attrs snapshot helper — matches the `Attrs` struct coupling surface.
impl From<&Attrs> for (Color, Color, bool, bool, bool, bool, bool) {
    fn from(a: &Attrs) -> Self {
        (
            a.foreground,
            a.background,
            a.bold,
            a.italic,
            a.underline,
            a.inverse,
            a.dim,
        )
    }
}

#[cfg(test)]
mod tests;
