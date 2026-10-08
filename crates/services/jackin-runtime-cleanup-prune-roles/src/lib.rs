//! jackin-runtime-cleanup-prune-roles: jackin runtime role-cache pruning.
//!
//! **Architecture Invariant:** T7.
//! Entry point: [`prune_roles::prune_roles`] —
//! remove the cached role repositories.
//!
//! Split out of `jackin-runtime` (S7 split 103): the guarded
//! role-cache removal (coordination gate plus the shared
//! prune-one-directory helper), decoupled from the
//! instance/cache/home pruning that stays in the hub. The old
//! `jackin_runtime::runtime::cleanup::prune_roles` path
//! keeps working through the hub re-export.

pub mod prune_roles;
