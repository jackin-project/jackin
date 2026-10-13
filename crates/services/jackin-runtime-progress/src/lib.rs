//! jackin-runtime-progress: host wiring for launch progress.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`progress::host_terminal`] — host terminal singleton.
//!
//! Bridges host I/O (terminal, clipboard, reveal/open, diagnostics
//! lines) into the launch progress surface owned by `jackin-launch`.
//! Split out of `jackin-runtime` (S7 split 51); the old
//! `jackin_runtime::runtime::progress::*` paths keep working through
//! a re-export shim.

pub mod progress;
