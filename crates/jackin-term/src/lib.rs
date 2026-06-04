//! jackin-term: owned terminal model for the jackin-capsule re-emitting PTY multiplexer.
//!
//! See `README.md` for the full engineering rationale.
//!
//! ## Pipeline
//!
//! ```text
//! vte (depend) → DamageGrid (build) → PassthroughEvents (build) → [WireDiff emit]
//! ```
//!
//! ## Status
//!
//! - Phase 0 (baseline): complete — vt100 coupling surface documented.
//! - Phase 1 (harness): complete — differential tester + corpus + fuzz target.
//! - Phase 2 (v0 grid): complete — DamageGrid with vte::Perform, dirty_spans(),
//!   typed PassthroughEvents. Full coupling surface implemented.
//! - Phase 3–5: not started.

pub mod cell;
pub mod damage;
pub mod grid;
pub mod passthrough;

pub use cell::{Attrs, Cell, Color};
pub use damage::{DirtySpans, DirtyTracker};
pub use grid::{DamageGrid, MouseProtocolEncoding, MouseProtocolMode};
pub use passthrough::{PassthroughBuffer, PassthroughEvent};
