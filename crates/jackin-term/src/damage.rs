//! BORROW: Dirty-span tracking design from Zellij's `OutputBuffer` / `changed_lines`
//! (MIT license, Zellij Contributors — <https://github.com/zellij-org/zellij>).
//! Concept: record which rows changed during PTY output processing, then emit only
//! those rows on the next render tick. Implementation is our own; the `DirtyTracker` +
//! `DirtySpans` shape is a simplified version adapted for our per-pane render path.
//!
//! Dirty-span tracking: records mutations at write time so the emit path
//! can walk only changed col-spans instead of diffing the whole grid.
//!
//! Phase 2 v0.1: a fixed row list plus preallocated row markers. This keeps
//! the common `process()` → `dirty_spans()` render path heap-free after grid
//! construction; Phase 4 can refine to per-row col-span ranges for even tighter
//! diffing.

// Frames that dirty more than this many distinct rows fall back to `All`.
// That keeps the enum small enough for hot-path returns while preserving the
// no-heap common path for focused-pane updates.
const MAX_TRACKED_DIRTY_ROWS: usize = 64;

/// Dirty-row tracker.  Records which rows were mutated since the last
/// call to `take()`.
#[derive(Debug)]
pub struct DirtyTracker {
    rows: DirtyRows,
    marked: Vec<bool>,
    all_dirty: bool,
}

impl DirtyTracker {
    pub fn new(row_count: u16) -> Self {
        Self {
            rows: DirtyRows::default(),
            marked: vec![false; row_count as usize],
            all_dirty: false,
        }
    }

    pub fn resize(&mut self, row_count: u16) {
        self.clear_rows();
        self.marked.resize(row_count as usize, false);
        self.mark_all();
    }

    /// Mark a single row dirty.
    pub fn mark_row(&mut self, row: u16) {
        if self.all_dirty {
            return;
        }

        let idx = row as usize;
        let Some(marked) = self.marked.get_mut(idx) else {
            self.mark_all();
            return;
        };
        if *marked {
            return;
        }

        if self.rows.push(row).is_err() {
            self.mark_all();
            return;
        }
        *marked = true;
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
            for row in rows.iter() {
                if let Some(marked) = self.marked.get_mut(row as usize) {
                    *marked = false;
                }
            }
            DirtySpans::Rows(rows)
        }
    }

    /// Whether any rows are marked dirty.
    pub fn is_dirty(&self) -> bool {
        self.all_dirty || !self.rows.is_empty()
    }

    fn clear_rows(&mut self) {
        for row in self.rows.iter() {
            if let Some(marked) = self.marked.get_mut(row as usize) {
                *marked = false;
            }
        }
        self.rows.clear();
    }
}

impl Default for DirtyTracker {
    fn default() -> Self {
        Self::new(0)
    }
}

#[derive(Debug, Clone)]
pub struct DirtyRows {
    rows: [u16; MAX_TRACKED_DIRTY_ROWS],
    len: usize,
}

impl DirtyRows {
    fn push(&mut self, row: u16) -> Result<(), ()> {
        if self.len == self.rows.len() {
            return Err(());
        }

        let insert_at = self.rows[..self.len]
            .binary_search(&row)
            .unwrap_or_else(|idx| idx);
        self.rows.copy_within(insert_at..self.len, insert_at + 1);
        self.rows[insert_at] = row;
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

    pub fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        self.rows[..self.len].iter().copied()
    }
}

impl Default for DirtyRows {
    fn default() -> Self {
        Self {
            rows: [0; MAX_TRACKED_DIRTY_ROWS],
            len: 0,
        }
    }
}

/// The set of rows changed since the last `take()`.
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
mod tests {
    use super::{DirtySpans, DirtyTracker};

    #[test]
    fn dirty_rows_are_sorted_and_deduplicated() {
        let mut dirty = DirtyTracker::new(10);

        dirty.mark_row(3);
        dirty.mark_row(1);
        dirty.mark_row(3);
        dirty.mark_row(2);

        let DirtySpans::Rows(rows) = dirty.take() else {
            panic!("expected row-specific dirty spans");
        };
        assert_eq!(rows.iter().collect::<Vec<_>>(), [1, 2, 3]);
        assert!(!dirty.is_dirty());
    }
}
