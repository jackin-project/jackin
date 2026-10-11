//! jackin-runtime-cleanup-timing: cleanup timing guard and failure reporting.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`timing::cleanup_timing`] — scoped timing guard for a
//! named cleanup phase.
//!
//! The drop guard ([`timing::CleanupTiming`]) that brackets a cleanup
//! phase with diagnostics timing, plus the failure reporter
//! ([`timing::cleanup_failure`]). Split out of `jackin-runtime` (S7
//! split 91); the old `jackin_runtime::runtime::cleanup::*` paths keep
//! working through item re-exports in `cleanup.rs`.

pub mod timing;
