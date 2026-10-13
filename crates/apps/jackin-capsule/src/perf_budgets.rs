//! Numeric allocation budgets for the R-perf-platform ratchet family.
//!
//! Values must match the `dhat::assert!` ceilings in
//! `tests/render_allocation.rs`. Shrink-only via `ratchet.toml` family `perf`.

/// Max heap blocks for a focused full-snapshot render after warmup.
pub const FOCUSED_FULL_SNAPSHOT_MAX_BLOCKS: usize = 3;
/// Max heap bytes for a focused full-snapshot render after warmup.
pub const FOCUSED_FULL_SNAPSHOT_MAX_BYTES: usize = 1024;
/// Max heap blocks for a focused borrowed-view render after warmup.
pub const FOCUSED_BORROWED_VIEW_MAX_BLOCKS: usize = 3;
/// Max heap bytes for a focused borrowed-view render after warmup.
pub const FOCUSED_BORROWED_VIEW_MAX_BYTES: usize = 1024;
