//! jackin-runtime-cleanup-absent-guards: absent-for-purge guards.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`absent_guards::ensure_role_resources_absent_for_purge`] —
//! refuse local-state purge while Docker resources still exist.
//!
//! Split out of `jackin-runtime` (S7 split 100): the pre-purge
//! Docker-absence checks over recorded instance resources, decoupled
//! from the bulk prune and filesystem teardown that stay in the hub.
//! The old `jackin_runtime::runtime::cleanup::`
//! `ensure_role_resources_absent_for_purge` path keeps working
//! through the hub re-export.

pub mod absent_guards;
