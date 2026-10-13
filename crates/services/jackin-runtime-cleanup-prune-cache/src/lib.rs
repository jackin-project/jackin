//! jackin-runtime-cleanup-prune-cache: jackin runtime shared-cache pruning.
//!
//! **Architecture Invariant:** T7.
//! Entry point: [`prune_cache::prune_cache`] —
//! remove the rebuildable shared cache.
//!
//! Split out of `jackin-runtime` (S7 split 104): the guarded
//! shared-cache removal (coordination gate plus the shared
//! prune-one-directory helper), decoupled from the
//! instance pruning that stays in the hub. The old
//! `jackin_runtime::runtime::cleanup::prune_cache` path
//! keeps working through the hub re-export.

pub mod prune_cache;
