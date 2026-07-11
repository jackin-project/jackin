//! jackin-term: terminal emulator grid, parser, and damage tracking.
//!
//! **Architecture Invariant:** T0.
//! Entry point: [`DamageGrid`] — terminal grid with damage tracking.

pub mod cell;
pub mod damage;
pub mod grid;
pub mod passthrough;
pub mod snapshot;
pub mod width;

pub use cell::{Attrs, Cell, Color, Hyperlink, UnderlineStyle};
pub use damage::{DirtySpans, DirtyTracker};
pub use grid::{DamageGrid, MouseProtocolEncoding, MouseProtocolMode, RowArena, RowWrap, ScrollOp};
pub use passthrough::{PassthroughBuffer, PassthroughEvent};
pub use snapshot::{GridPatch, GridSnapshot, GridView, SnapCell};
pub use width::{Osc8Policy, SupportedSgr, VirtualTerminalProfile, display_width};
