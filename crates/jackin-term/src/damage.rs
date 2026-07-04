// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! BORROW: Dirty-span tracking design from Zellij's `OutputBuffer` / `changed_lines`
//! (MIT license, Zellij Contributors — <https://github.com/zellij-org/zellij>).
//! Concept: record which rows changed during PTY output processing, then emit only
//! those rows on the next render tick. Implementation is our own; the `DirtyTracker` +
//! `DirtySpans` shape is a simplified version adapted for our per-pane render path.
//!
//! Dirty-span tracking: records mutations at write time so the emit path
//! can walk only changed col-spans instead of diffing the whole grid.
//!
//! Phase 2 v0.2: a fixed row list carrying one merged column span per row.
//! This keeps the common `process()` → `dirty_spans()` render path heap-free
//! after grid construction while letting the live emit path write only the
//! cells that changed.

// Frames that dirty more than this many distinct rows fall back to `All`.
// That keeps the enum small enough for hot-path returns while preserving the
// no-heap common path for focused-pane updates.
const MAX_TRACKED_DIRTY_ROWS: usize = 64;

/// Dirty-row tracker.  Records which rows were mutated since the last
/// call to `take()`.
#[derive(Debug)]
pub struct DirtyTracker {
    rows: DirtyRows,
    all_dirty: bool,
}

impl DirtyTracker {
    pub fn new(_row_count: u16) -> Self {
        Self {
            rows: DirtyRows::default(),
            all_dirty: false,
        }
    }

    pub fn resize(&mut self, row_count: u16) {
        let _ = row_count;
        self.clear_rows();
        self.mark_all();
    }

    /// Mark a single row dirty.
    pub fn mark_row(&mut self, row: u16) {
        self.mark_range(row, 0, u16::MAX);
    }

    /// Mark a single cell dirty.
    pub fn mark_cell(&mut self, row: u16, col: u16) {
        self.mark_range(row, col, col.saturating_add(1));
    }

    /// Mark a half-open column range dirty on one row.
    pub fn mark_range(&mut self, row: u16, start_col: u16, end_col: u16) {
        if self.all_dirty {
            return;
        }
        if start_col >= end_col {
            return;
        }
        if self.rows.push_or_merge(row, start_col, end_col).is_err() {
            self.mark_all();
        }
    }

    /// Mark every row dirty (e.g. after a full screen clear or resize).
    pub fn mark_all(&mut self) {
        self.clear_rows();
        self.all_dirty = true;
    }

    /// Drain and return dirty rows, sorted ascending.
    /// After this call the tracker is clean.
    pub fn take(&mut self) -> DirtySpans {
        if self.all_dirty {
            self.all_dirty = false;
            DirtySpans::All
        } else {
            let rows = std::mem::take(&mut self.rows);
            DirtySpans::Rows(rows)
        }
    }

    /// Whether any rows are marked dirty.
    pub fn is_dirty(&self) -> bool {
        self.all_dirty || !self.rows.is_empty()
    }

    fn clear_rows(&mut self) {
        self.rows.clear();
    }
}

impl Default for DirtyTracker {
    fn default() -> Self {
        Self::new(0)
    }
}

/// One dirty span on a row. `end_col == u16::MAX` means "through grid width".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtySpan {
    pub row: u16,
    pub start_col: u16,
    pub end_col: u16,
}

impl DirtySpan {
    #[must_use]
    pub const fn full_row(row: u16) -> Self {
        Self {
            row,
            start_col: 0,
            end_col: u16::MAX,
        }
    }

    #[must_use]
    pub const fn is_full_row(self) -> bool {
        self.start_col == 0 && self.end_col == u16::MAX
    }

    fn merge(&mut self, start_col: u16, end_col: u16) {
        self.start_col = self.start_col.min(start_col);
        self.end_col = self.end_col.max(end_col);
    }
}

#[derive(Debug, Clone)]
pub struct DirtyRows {
    rows: [DirtySpan; MAX_TRACKED_DIRTY_ROWS],
    len: usize,
}

impl DirtyRows {
    fn push_or_merge(&mut self, row: u16, start_col: u16, end_col: u16) -> Result<(), ()> {
        if let Ok(idx) = self.rows[..self.len].binary_search_by_key(&row, |span| span.row) {
            self.rows[idx].merge(start_col, end_col);
            return Ok(());
        }

        if self.len == self.rows.len() {
            return Err(());
        }

        let insert_at = self.rows[..self.len]
            .binary_search_by_key(&row, |span| span.row)
            .unwrap_or_else(|idx| idx);
        self.rows.copy_within(insert_at..self.len, insert_at + 1);
        self.rows[insert_at] = DirtySpan {
            row,
            start_col,
            end_col,
        };
        self.len += 1;
        Ok(())
    }

    fn clear(&mut self) {
        self.len = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn contains(&self, row: u16) -> bool {
        self.rows[..self.len]
            .binary_search_by_key(&row, |span| span.row)
            .is_ok()
    }

    pub fn as_slice(&self) -> &[DirtySpan] {
        &self.rows[..self.len]
    }

    pub fn iter(&self) -> impl Iterator<Item = DirtySpan> + '_ {
        self.as_slice().iter().copied()
    }
}

impl Default for DirtyRows {
    fn default() -> Self {
        Self {
            rows: [DirtySpan::full_row(0); MAX_TRACKED_DIRTY_ROWS],
            len: 0,
        }
    }
}

/// The set of rows changed since the last `take()`.
#[expect(
    clippy::large_enum_variant,
    reason = "dirty spans keep a fixed inline row array so the render hot path stays heap-free"
)]
#[derive(Debug, Clone)]
pub enum DirtySpans {
    /// Every row is dirty — e.g. after resize or full clear.
    All,
    /// Specific rows (sorted ascending).
    Rows(DirtyRows),
}

impl DirtySpans {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::All => false,
            Self::Rows(v) => v.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests;
