//! jackin-runtime-isolation: mount-isolation facade over jackin-isolation.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`isolation::materialize::materialize_workspace`] — bind-spec materialization.
//!
//! Re-exports the `jackin-isolation` strategy sub-modules (`branch`,
//! `cleanup`, `materialize`, `state`, `finalize`, `git_inspect`,
//! `safe_remove`) plus the `MountIsolation` enum from `jackin-core`.
//! Split out of `jackin-runtime` (S7 split 41); the old
//! `jackin_runtime::isolation::*` paths keep working through a re-export
//! shim.

pub mod isolation;
