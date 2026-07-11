//! jackin-isolation: workspace isolation materialization and mounts.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`materialize`] — workspace materialization entry.

pub mod branch;
pub mod cleanup;
pub mod finalize;
pub mod git_inspect;
pub mod materialize;
pub mod state;

pub use jackin_core::MountIsolation;
pub use jackin_core::ParseMountIsolationError;
