//! jackin-runtime-drift: workspace isolation drift detection.
//!
//! **Architecture Invariant:** T7.
//! Entry point: [`drift::detect_workspace_edit_drift`] — drift classifier.
//!
//! Finds mounts whose `src` changed while containers hold preserved
//! isolation state. Split out of `jackin-runtime` (S7 split 55); the
//! old `jackin_runtime::runtime::drift::*` paths keep working
//! through a re-export shim.

pub mod drift;
