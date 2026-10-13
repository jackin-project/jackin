//! jackin-runtime-launch-capsule-setup: launch capsule setup.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`capsule_setup::capsule_config`] — capsule config assembly.
//!
//! Builds the capsule config and socket dir, resolves per-instance auth
//! bindings, models, and efforts, and owns the private host env-file
//! transport. Split out of `jackin-runtime` (S7 split 65); the old
//! `jackin_runtime::runtime::launch::capsule_setup::*` paths keep
//! working through a re-export shim.

pub mod capsule_setup;
