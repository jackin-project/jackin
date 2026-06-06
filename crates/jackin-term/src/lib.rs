//! jackin-term: owned terminal model for the jackin-capsule re-emitting PTY multiplexer.
//!
//! See `README.md` for the full engineering rationale.
//!
//! ## Pipeline
//!
//! ```text
//! vte (depend) → DamageGrid (build) → GridSnapshot (dump) → Ratatui PaneBodyWidget (emit)
//! ```
//!
//! jackin-term owns the per-pane terminal *model*; the capsule paints each
//! `GridSnapshot` into a Ratatui buffer and lets the Ratatui `SocketBackend`
//! diff and emit the bytes. There is no jackin-term ANSI emitter.
//!
//! ## Status
//!
//! - Phase 0 (baseline): complete — vt100 coupling surface documented.
//! - Phase 1 (harness): complete — differential tester + corpus + fuzz target.
//! - Phase 2 (v0 grid): complete — `DamageGrid` with `vte::Perform`, `dirty_spans()`,
//!   typed `PassthroughEvents`. Full coupling surface implemented.
//! - Phase 3 (capsule feature flag): complete — `jackin-term` feature routes live render through `DamageGrid`; scrollback fallback to vt100 (Phase 4).
//! - Phase 4 (optimize): in progress — `Cell::contents` uses `CompactString` (no heap alloc for ≤24 byte graphemes, covers ASCII + most Unicode). Ghostty `PageList` arena pending.
//! - Phase 5: not started.

pub mod cell;
pub mod damage;
pub mod grid;
pub mod passthrough;
pub mod snapshot;

pub use cell::{Attrs, Cell, Color};
pub use damage::{DirtySpans, DirtyTracker};
pub use grid::{DamageGrid, MouseProtocolEncoding, MouseProtocolMode};
pub use passthrough::{PassthroughBuffer, PassthroughEvent};
pub use snapshot::{GridSnapshot, SnapCell};
