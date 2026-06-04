//! Dirty-span tracking: records mutations at write time so the emit path
//! can walk only changed col-spans instead of diffing the whole grid.
//!
//! Phase 2 v0: simple `HashSet<u16>` of dirty rows. Phase 4 refines to
//! per-row col-span ranges for tighter diffing.

use std::collections::BTreeSet;

/// Dirty-row tracker.  Records which rows were mutated since the last
/// call to `take()`.
#[derive(Debug, Default)]
pub struct DirtyTracker {
    rows: BTreeSet<u16>,
    all_dirty: bool,
}

impl DirtyTracker {
    /// Mark a single row dirty.
    pub fn mark_row(&mut self, row: u16) {
        if !self.all_dirty {
            self.rows.insert(row);
        }
    }

    /// Mark every row dirty (e.g. after a full screen clear or resize).
    pub fn mark_all(&mut self) {
        self.all_dirty = true;
        self.rows.clear();
    }

    /// Drain and return dirty rows, sorted ascending.
    /// After this call the tracker is clean.
    pub fn take(&mut self) -> DirtySpans {
        if self.all_dirty {
            self.all_dirty = false;
            DirtySpans::All
        } else {
            DirtySpans::Rows(std::mem::take(&mut self.rows).into_iter().collect())
        }
    }

    /// Whether any rows are marked dirty.
    pub fn is_dirty(&self) -> bool {
        self.all_dirty || !self.rows.is_empty()
    }
}

/// The set of rows changed since the last `take()`.
#[derive(Debug, Clone)]
pub enum DirtySpans {
    /// Every row is dirty — e.g. after resize or full clear.
    All,
    /// Specific rows (sorted ascending).
    Rows(Vec<u16>),
}

impl DirtySpans {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::All => false,
            Self::Rows(v) => v.is_empty(),
        }
    }
}
