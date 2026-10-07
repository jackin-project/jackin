//! jackin-runtime-prune-output: prune and cleanup terminal output.
//!
//! **Architecture Invariant:** T0.
//! Entry point: [`prune_output::start`] — pending status row.
//!
//! Formatted prune and cleanup terminal output shared by runtime
//! and diagnostics. Split out of `jackin-runtime` (S7 split 52);
//! the old `jackin_runtime::runtime::prune_output::*` paths keep
//! working through a re-export shim.

pub mod prune_output;
