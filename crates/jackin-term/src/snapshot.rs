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

use crate::cell::{Attrs, Cell, Color};

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
            text: cell.contents().to_string(),
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
mod tests {
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
            inverse: false,
            dim: false,
        };
        assert!(c.is_blank());
        assert!(!c.has_contents());
    }

    #[test]
    fn snap_cell_non_blank() {
        let c = SnapCell {
            text: "A".to_string(),
            is_wide: false,
            is_wide_continuation: false,
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
            dim: false,
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
}
