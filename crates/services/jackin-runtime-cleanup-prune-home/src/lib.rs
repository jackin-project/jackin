//! jackin-runtime-cleanup-prune-home: jackin runtime home directory pruning.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`prune_home::prune_jackin_home`] —
//! remove the remaining runtime home state.
//!
//! Split out of `jackin-runtime` (S7 split 101): the guarded
//! runtime-home removal (coordination gate plus prune-output rows
//! plus owned-validated deletion), decoupled from the
//! instance/role/cache pruning that stays in the hub. The old
//! `jackin_runtime::runtime::cleanup::prune_jackin_home` path
//! keeps working through the hub re-export.

pub mod prune_home;
