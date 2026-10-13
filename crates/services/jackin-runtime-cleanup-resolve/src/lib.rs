//! jackin-runtime-cleanup-resolve: cleanup handle resolution.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`resolve::resolve_cleanup_handles_for_state`] —
//! handle snapshot.
//!
//! Ownership-checked container-handle resolution against recorded
//! instance state. Split out of `jackin-runtime` (S7 split 82);
//! the old `jackin_runtime::runtime::cleanup::*` paths keep
//! working through item re-exports in `cleanup.rs`.

pub mod resolve;
