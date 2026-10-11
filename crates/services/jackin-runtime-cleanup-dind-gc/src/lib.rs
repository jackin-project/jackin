//! jackin-runtime-cleanup-dind-gc: orphaned `DinD`/network garbage collection.
//!
//! **Architecture Invariant:** T7.
//! Entry point: [`dind_gc::gc_orphaned_resources`] — best-effort sweep of
//! orphaned `DinD` sidecars, their cert volumes, role networks, and the
//! kept prewarm sidecar.
//!
//! `DinD` sidecar enumeration ([`dind_gc::collect_labeled_dind`]), orphan
//! classification ([`dind_gc::filter_orphaned_dind`]), orphaned-network GC
//! ([`dind_gc::gc_orphaned_networks`]), and prewarm-sidecar GC
//! ([`dind_gc::gc_orphaned_prewarm_dind`]). Split out of `jackin-runtime`
//! (S7 split 93); the old `jackin_runtime::runtime::cleanup::*` paths keep
//! working through item re-exports in `cleanup.rs`.

pub mod dind_gc;
