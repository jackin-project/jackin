//! jackin-runtime-launch-debug-envs: debug runtime env helper.
//!
//! **Architecture Invariant:** T0.
//! Entry point: [`debug_envs::debug_runtime_envs`] —
//! debug env strings for the launch run args.
//!
//! Split out of `jackin-runtime` (S7 split 115): the debug
//! env helper in the launch runtime `run` path.
//! The hub keeps a `pub(crate)` re-export; the step
//! stays exercised through the same launch path plus
//! the hub launch suite.

pub mod debug_envs;
