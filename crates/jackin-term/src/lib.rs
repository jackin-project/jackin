//! jackin-term: owned terminal model for the jackin-capsule re-emitting PTY multiplexer.
//!
//! See `README.md` for the full engineering rationale.
//!
//! ## Pipeline
//!
//! ```text
//! vte (depend) → DamageGrid (build) → GridView/GridPatch (borrow) → capsule emit
//! ```
//!
//! jackin-term owns the per-pane terminal *model*; the capsule paints each
//! borrowed `GridView` into a Ratatui buffer for fallback frames or emits dirty
//! `GridPatch` spans directly. There is no jackin-term ANSI emitter.
//!
//! ## Status
//!
//! - Phase 0 (baseline): complete — terminal-model coupling surface documented.
//! - Phase 1 (harness): complete — conformance replay + corpus + fuzz target.
//! - Phase 2 (v0 grid): complete — `DamageGrid` with `vte::Perform`, `dirty_spans()`,
//!   typed `PassthroughEvents`. Full coupling surface implemented.
//! - Phase 3 (capsule cutover): complete — `jackin-term` routes live render through `DamageGrid`.
//! - Phase 4 (optimize): in progress — `Cell::contents` uses `CompactString` (no heap alloc for ≤24 byte graphemes, covers ASCII + most Unicode). Ghostty `PageList` arena pending.
//! - Phase 5: not started.

pub mod cell;
pub mod damage;
pub mod grid;
pub mod passthrough;
pub mod snapshot;
pub mod width;

pub use cell::{Attrs, Cell, Color};
pub use damage::{DirtySpans, DirtyTracker};
pub use grid::{DamageGrid, MouseProtocolEncoding, MouseProtocolMode, RowArena, RowWrap};
pub use passthrough::{PassthroughBuffer, PassthroughEvent};
pub use snapshot::{GridPatch, GridSnapshot, GridView, SnapCell};
pub use width::{VirtualTerminalProfile, display_width};
