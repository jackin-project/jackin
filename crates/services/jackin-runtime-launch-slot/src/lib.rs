//! jackin-runtime-launch-slot: name slots and github preflight.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`launch_slot::claim_container_name`] — slot claims.
//!
//! Container name slot claims (unique + known-name reclaim) with
//! flock-backed locks, plus the github token-present preflight and
//! `[github.env]` resolution. Split out of `jackin-runtime` (S7
//! split 77); the old `jackin_runtime::runtime::launch::launch_slot::*`
//! paths keep working through a module re-export in `launch.rs`.

pub mod launch_slot;
