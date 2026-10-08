//! jackin-runtime-cleanup-prune-dir: jackin runtime prune-one-directory helper.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`prune_dir::prune_dir`] —
//! delete one directory with prune-output reporting.
//!
//! Split out of `jackin-runtime` (S7 split 102): the shared
//! guarded directory removal (prune-output rows plus
//! owned-validated deletion plus cleanup-failure recording),
//! decoupled from the role/cache/instance pruning that stays
//! in the hub. The old
//! `jackin_runtime::runtime::cleanup::prune_dir` path
//! keeps working through the hub re-export.

pub mod prune_dir;
