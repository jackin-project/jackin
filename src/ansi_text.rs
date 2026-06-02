//! ANSI escape code stripping and plain-text width calculation for terminal output.
//!
//! Re-exports the canonical implementation from `jackin_tui::ansi_text`.
//!
//! Not responsible for: rendering, color selection, or any write to a terminal fd.

pub use jackin_tui::ansi_text::*;
