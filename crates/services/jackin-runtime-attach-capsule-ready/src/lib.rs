//! jackin-runtime-attach-capsule-ready: readiness waits.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`capsule_ready::wait_for_dind`] — `DinD` wait.
//!
//! Capsule daemon readiness waits and `dind` warmup waits.
//! Split out of `jackin-runtime` (S7 split 74); the old
//! `jackin_runtime::runtime::attach::capsule_ready::*` paths
//! keep working through a module re-export in `attach.rs`.

pub mod capsule_ready;
